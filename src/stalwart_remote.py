# Executed only on the connected Linux host for the Stalwart Setup page.
# Read-only: it inspects the container, DNS and network, and changes nothing.
import ipaddress
import json
import random
import re
import socket
import ssl
import struct
import subprocess
import sys
import urllib.request

LIMIT = 2 * 1024 * 1024
rows = []
RESOLVERS = ('1.1.1.1', '8.8.8.8')
TYPES = {'A': 1, 'CNAME': 5, 'PTR': 12, 'MX': 15, 'TXT': 16, 'AAAA': 28, 'SRV': 33}
# Cloudflare's published proxy ranges: a mail name resolving here is
# orange-clouded, and SMTP/IMAP cannot pass through the proxy.
CLOUDFLARE = [ipaddress.ip_network(net) for net in (
    '173.245.48.0/20', '103.21.244.0/22', '103.22.200.0/22', '103.31.4.0/22',
    '141.101.64.0/18', '108.162.192.0/18', '190.93.240.0/20', '188.114.96.0/20',
    '197.234.240.0/22', '198.41.128.0/17', '162.158.0.0/15', '104.16.0.0/13',
    '104.24.0.0/14', '172.64.0.0/13', '131.0.72.0/22')]
ANSI = re.compile(r'\x1b\[[0-9;]*[A-Za-z]')

def add(*cells):
    rows.append([str(cell) for cell in cells])

def check(key, status, detail=''):
    add('check', key, status, detail)

def run(args, timeout=20):
    try:
        process = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, timeout=timeout)
    except subprocess.TimeoutExpired:
        return 124, 'timeout'
    except OSError as error:
        return 127, str(error)
    return process.returncode, process.stdout[-LIMIT:].decode('utf-8', 'replace')

def encode_name(name):
    labels = name.rstrip('.').split('.')
    return b''.join(bytes([len(label)]) + label.encode('ascii') for label in labels) + b'\0'

def read_name(message, offset):
    labels, end, hops = [], None, 0
    while True:
        length = message[offset]
        if length & 0xC0 == 0xC0:
            if end is None:
                end = offset + 2
            offset = ((length & 0x3F) << 8) | message[offset + 1]
            hops += 1
            if hops > 30:
                raise ValueError('loop')
            continue
        if length == 0:
            offset += 1
            break
        labels.append(message[offset + 1:offset + 1 + length].decode('ascii', 'replace'))
        offset += 1 + length
    return '.'.join(labels).lower(), end if end is not None else offset

def query(name, kind, qclass=1, servers=RESOLVERS):
    # Asks public resolvers directly (not the server's cache), so a record
    # just edited in Cloudflare is seen the way the rest of the world sees it.
    ident = random.randrange(65536)
    packet = (struct.pack('>HHHHHH', ident, 0x0100, 1, 0, 0, 1) + encode_name(name)
              + struct.pack('>HH', TYPES[kind], qclass)
              + b'\0' + struct.pack('>HHIH', 41, 4096, 0, 0))
    last = 'no answer'
    for server in servers:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
                sock.settimeout(2.5)
                sock.sendto(packet, (server, 53))
                message, _ = sock.recvfrom(8192)
            if len(message) < 12 or struct.unpack('>H', message[:2])[0] != ident:
                continue
            flags, questions, answers_count = struct.unpack('>HHH', message[2:8])
            rcode = flags & 0xF
            if rcode == 3:
                return []
            if rcode:
                last = 'rcode ' + str(rcode)
                continue
            offset = 12
            for _ in range(questions):
                offset = read_name(message, offset)[1] + 4
            answers = []
            for _ in range(answers_count):
                offset = read_name(message, offset)[1]
                rtype, _, _, length = struct.unpack('>HHIH', message[offset:offset + 10])
                offset += 10
                data = message[offset:offset + length]
                if rtype == 1 and length == 4:
                    answers.append(('A', socket.inet_ntoa(data)))
                elif rtype == 28 and length == 16:
                    answers.append(('AAAA', socket.inet_ntop(socket.AF_INET6, data)))
                elif rtype == 5:
                    answers.append(('CNAME', read_name(message, offset)[0]))
                elif rtype == 12:
                    answers.append(('PTR', read_name(message, offset)[0]))
                elif rtype == 15:
                    answers.append(('MX', read_name(message, offset + 2)[0]))
                elif rtype == 16:
                    parts, index = [], 0
                    while index < length:
                        size = data[index]
                        parts.append(data[index + 1:index + 1 + size].decode('utf-8', 'replace'))
                        index += 1 + size
                    answers.append(('TXT', ''.join(parts)))
                elif rtype == 33:
                    port = struct.unpack('>H', data[4:6])[0]
                    answers.append(('SRV', str(port) + ' ' + read_name(message, offset + 6)[0]))
                offset += length
            return answers
        except (OSError, ValueError, struct.error, IndexError) as error:
            last = str(error)
    raise ValueError(last)

def values(answers, kind):
    return [value for rtype, value in answers if rtype == kind]

def proxied(addresses):
    for address in addresses:
        try:
            if any(ipaddress.ip_address(address) in net for net in CLOUDFLARE):
                return True
        except ValueError:
            pass
    return False

def public_ip(ssh_address):
    # Cloudflare's resolver answers `whoami.cloudflare` (class CHAOS) with
    # the address the query came from: the server's public outbound IP.
    try:
        found = values(query('whoami.cloudflare', 'TXT', 3, ('1.1.1.1',)), 'TXT')
        if found:
            return str(ipaddress.ip_address(found[0]))
    except ValueError:
        pass
    try:
        return str(ipaddress.ip_address(ssh_address))
    except ValueError:
        try:
            return socket.gethostbyname(ssh_address)
        except OSError:
            return ''

def check_points_here(key, name, server_ip):
    try:
        addresses = values(query(name, 'A'), 'A')
    except ValueError as error:
        check(key, 'unknown', str(error))
        return
    if not addresses:
        check(key, 'fail', 'missing')
    elif server_ip and server_ip in addresses:
        check(key, 'ok', ', '.join(addresses))
    elif proxied(addresses):
        check(key, 'fail', 'proxied')
    else:
        check(key, 'fail', ', '.join(addresses))

def check_txt(key, name, prefix, required):
    try:
        found = [text for text in values(query(name, 'TXT'), 'TXT') if text.lower().startswith(prefix)]
    except ValueError as error:
        check(key, 'unknown', str(error))
        return
    if len(found) > 1:
        check(key, 'fail', 'duplicate')
    elif found:
        check(key, 'ok', found[0])
    else:
        check(key, 'fail' if required else 'warn', 'missing')

def check_spf(domain, hostname, server_ip):
    # One SPF record is not enough: it must also let this server send.
    try:
        found = [text for text in values(query(domain, 'TXT'), 'TXT') if text.lower().startswith('v=spf1')]
    except ValueError as error:
        check('spf_domain', 'unknown', str(error))
        return
    if len(found) > 1:
        check('spf_domain', 'fail', 'duplicate')
    elif not found:
        check('spf_domain', 'fail', 'missing')
    else:
        allowed = {'mx', 'mx:' + domain, 'a:' + hostname, 'ip4:' + server_ip}
        terms = [term.lstrip('+') for term in found[0].lower().split()[1:]]
        if any(term in allowed for term in terms):
            check('spf_domain', 'ok', found[0])
        else:
            check('spf_domain', 'fail', 'unauthorized:' + found[0])

def check_srv(key, name, hostname):
    try:
        found = values(query(name, 'SRV'), 'SRV')
    except ValueError as error:
        check(key, 'unknown', str(error))
        return
    if any(value.split(' ', 1)[-1] == hostname for value in found):
        check(key, 'ok', ', '.join(found))
    else:
        check(key, 'warn', ', '.join(found) or 'missing')

def http_live(url, verify=False):
    context = ssl.create_default_context()
    if not verify:
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
    try:
        with urllib.request.urlopen(url, timeout=5, context=context) as response:
            return response.status == 200
    except Exception:
        return False

def published(bindings, port):
    found = bindings.get(str(port) + '/tcp') or []
    public = [b for b in found if b.get('HostIp', '') in ('', '0.0.0.0', '::')]
    local = [b for b in found if b.get('HostIp', '') in ('127.0.0.1', '::1')]
    return public, local

def inspect(container):
    code, output = run(['docker', 'inspect', '--type', 'container', container])
    if code:
        check('container', 'fail', 'not-found')
        return None
    info = json.loads(output)[0]
    running = info.get('State', {}).get('Running', False)
    check('container', 'ok' if running else 'fail', info.get('Config', {}).get('Image', ''))
    destinations = {mount.get('Destination') for mount in info.get('Mounts') or []}
    missing = [path for path in ('/etc/stalwart', '/var/lib/stalwart') if path not in destinations]
    check('mounts', 'warn' if missing else 'ok', ' '.join(missing))
    return info

def ports(info):
    host_network = info.get('HostConfig', {}).get('NetworkMode') == 'host'
    bindings = info.get('NetworkSettings', {}).get('Ports') or {}
    https_port = None
    for port in (25, 465, 993, 443):
        if host_network:
            check('port_' + str(port), 'ok', 'host')
            https_port = 443
            continue
        public, _ = published(bindings, port)
        if any(b.get('HostPort') == str(port) for b in public):
            check('port_' + str(port), 'ok', 'public')
            if port == 443:
                https_port = 443
        elif public:
            check('port_' + str(port), 'warn', 'remapped:' + public[0].get('HostPort', ''))
            if port == 443:
                https_port = int(public[0].get('HostPort'))
        else:
            check('port_' + str(port), 'fail', 'not-published')
    # The wizard is reached through the SSH tunnel: on the host's loopback
    # port when 8080 is published there, else straight at the container.
    target = ''
    if host_network:
        target = '127.0.0.1:8080'
        check('port_8080', 'ok', 'host')
    else:
        public, local = published(bindings, 8080)
        if public:
            target = '127.0.0.1:' + public[0].get('HostPort', '8080')
            check('port_8080', 'warn', 'public')
        elif local:
            target = '127.0.0.1:' + local[0].get('HostPort', '8080')
            check('port_8080', 'ok', 'loopback')
        else:
            for network in (info.get('NetworkSettings', {}).get('Networks') or {}).values():
                if network.get('IPAddress'):
                    target = network['IPAddress'] + ':8080'
                    break
            check('port_8080', 'ok' if target else 'fail', 'internal' if target else 'not-published')
    if target:
        add('wizard_target', target)
    return https_port, target

def mode(container, info, https_port, target):
    running = info.get('State', {}).get('Running', False)
    config = 'unknown'
    if running:
        code, output = run(['docker', 'exec', container, 'ls', '/etc/stalwart/config.json'])
        config = 'yes' if code == 0 else 'no' if 'No such file' in output else 'unknown'
    add('config', config)
    https = bool(running and https_port and http_live('https://127.0.0.1:%d/healthz/live' % https_port))
    wizard = bool(running and target and http_live('http://%s/healthz/live' % target))
    if https:
        state = 'running'
    elif config == 'yes':
        state = 'needs_restart'
    elif wizard:
        state = 'bootstrap'
    else:
        state = 'down'
    add('state', state)
    if state == 'bootstrap':
        # Only the newest block is valid: every bootstrap start prints a new password.
        code, output = run(['docker', 'logs', '--tail', '3000', container])
        lines = [ANSI.sub('', line) for line in output.splitlines()]
        starts = [index for index, line in enumerate(lines) if 'bootstrap mode' in line]
        if starts:
            add('bootstrap', '\n'.join(lines[starts[-1]:starts[-1] + 9]))
    return https, https_port

def certificate(hostname, https_port):
    # Verifies the chain and the name against the system CAs, as a mail
    # client would; a self-signed placeholder means ACME has not finished.
    context = ssl.create_default_context()
    try:
        with socket.create_connection(('127.0.0.1', https_port), timeout=6) as sock:
            with context.wrap_socket(sock, server_hostname=hostname) as tls:
                issuer = dict(item[0] for item in tls.getpeercert().get('issuer', ()))
                check('certificate', 'ok', issuer.get('organizationName') or issuer.get('commonName', ''))
    except ssl.SSLCertVerificationError as error:
        check('certificate', 'fail', error.verify_message or str(error))
    except (OSError, ssl.SSLError) as error:
        check('certificate', 'unknown', str(error))

def outbound_25():
    try:
        with socket.create_connection(('gmail-smtp-in.l.google.com', 25), timeout=6) as sock:
            sock.settimeout(6)
            banner = sock.recv(512).decode('ascii', 'replace')
        check('outbound_25', 'ok' if banner.startswith('220') else 'warn', banner.strip()[:80])
    except OSError:
        check('outbound_25', 'fail', 'blocked')

def main(container, hostname, domain, ssh_address):
    server_ip = public_ip(ssh_address)
    add('server_ip', server_ip)
    info = inspect(container)
    https, https_port = False, None
    if info is not None:
        https_port, target = ports(info)
        https, https_port = mode(container, info, https_port, target)
    outbound_25()
    if server_ip:
        try:
            reverse = ipaddress.ip_address(server_ip).reverse_pointer
            found = values(query(reverse, 'PTR'), 'PTR')
            check('ptr', 'ok' if hostname in found else 'fail', ', '.join(found) or 'missing')
        except ValueError as error:
            check('ptr', 'unknown', str(error))
    else:
        check('ptr', 'unknown', 'no-ip')
    check_points_here('a_record', hostname, server_ip)
    try:
        found = values(query(domain, 'MX'), 'MX')
        check('mx', 'ok' if hostname in found else 'fail', ', '.join(found) or 'missing')
    except ValueError as error:
        check('mx', 'unknown', str(error))
    check_spf(domain, hostname, server_ip)
    check_txt('spf_host', hostname, 'v=spf1', False)
    for key, label in (('autoconfig', 'autoconfig'), ('autodiscover', 'autodiscover'),
                       ('mta_sts', 'mta-sts'), ('ua_auto_config', 'ua-auto-config')):
        check_points_here(key, label + '.' + domain, server_ip)
    check_txt('dmarc', '_dmarc.' + domain, 'v=dmarc1', False)
    check_txt('mta_sts_txt', '_mta-sts.' + domain, 'v=stsv1', False)
    check_txt('tls_rpt', '_smtp._tls.' + domain, 'v=tlsrptv1', False)
    check_txt('ua_txt', '_ua-auto-config.' + domain, 'v=uaac1', False)
    check_srv('srv_imaps', '_imaps._tcp.' + domain, hostname)
    check_srv('srv_submissions', '_submissions._tcp.' + domain, hostname)
    if https:
        check('https', 'ok')
        certificate(hostname, https_port)
    else:
        check('https', 'fail' if info is not None else 'unknown')

try:
    raw = sys.stdin.buffer.read(100000)
    lines = raw.decode('ascii').splitlines()
    fields = [bytes.fromhex(line).decode('utf-8') for line in lines[1:]]
    if lines[0] != 'check' or len(fields) != 4:
        raise ValueError('Permintaan pemeriksaan tidak dikenal.')
    main(*fields)
    output = 'AWAN_LIVE_V1\n' + ''.join('\t'.join(str(cell).encode().hex() for cell in row) + '\n' for row in rows) + 'END\n'
    sys.stdout.write(output)
except Exception as error:
    print('[Awan] ' + str(error), file=sys.stderr)
    sys.exit(1)
