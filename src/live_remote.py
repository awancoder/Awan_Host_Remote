# Executed only on the connected Linux host. No installation or persistent agent.
import os
import platform
import selectors
import shutil
import signal
import subprocess
import sys
import time

LIMIT = 2 * 1024 * 1024
rows = []

def run(args, env=None):
    process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, env=env, start_new_session=True)
    output, errors = bytearray(), bytearray()
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, output)
    selector.register(process.stderr, selectors.EVENT_READ, errors)
    deadline = time.monotonic() + 15
    try:
        while selector.get_map():
            if time.monotonic() > deadline:
                raise ValueError('Perintah server melewati batas 15 detik.')
            for key, _ in selector.select(0.2):
                block = os.read(key.fileobj.fileno(), 8192)
                if not block:
                    selector.unregister(key.fileobj)
                else:
                    key.data.extend(block)
                    if len(output) + len(errors) > LIMIT:
                        raise ValueError('Output melebihi 2 MiB. Persempit pilihan data.')
        code = process.wait(timeout=max(0.1, deadline - time.monotonic()))
        if code:
            raise ValueError(errors[:8192].decode('utf-8', 'replace') or 'Perintah gagal: exit ' + str(code))
        return output.decode('utf-8')
    finally:
        selector.close()
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        process.stdout.close()
        process.stderr.close()

def root_command(args):
    return args if os.geteuid() == 0 else ['sudo', '-n', '--'] + args

def add(key, value):
    rows.append([key, str(value)])

def cpu_sample():
    with open('/proc/stat') as source:
        values = list(map(int, source.readline().split()[1:9]))
    return sum(values), values[3] + values[4]

VIRTUAL_FSTYPES = {
    'proc', 'sysfs', 'devtmpfs', 'devpts', 'tmpfs', 'cgroup', 'cgroup2',
    'pstore', 'bpf', 'tracefs', 'debugfs', 'securityfs', 'mqueue',
    'hugetlbfs', 'autofs', 'binfmt_misc', 'overlay', 'squashfs', 'ramfs',
    'fuse.portal', 'fusectl', 'configfs', 'rpc_pipefs', 'nsfs', 'efivarfs',
}

def unescape_mount(value):
    return value.replace('\\040', ' ').replace('\\011', '\t').replace('\\012', '\n').replace('\\134', '\\')

def summary():
    add('hostname', platform.node())
    add('kernel', platform.release() + ' / ' + platform.machine())
    add('cores', os.cpu_count() or 'Tidak tersedia')
    try:
        with open('/etc/os-release') as source:
            info = dict(line.rstrip().split('=', 1) for line in source if '=' in line)
        add('os', info.get('PRETTY_NAME', platform.system()).strip('"'))
    except OSError as error:
        add('os', str(error))
    try:
        model = None
        with open('/proc/cpuinfo') as source:
            for line in source:
                if ':' not in line:
                    continue
                key, _, value = line.partition(':')
                if key.strip().lower() in ('model name', 'hardware', 'model', 'cpu model'):
                    model = value.strip()
                    break
        add('cpu_model', model or 'Tidak tersedia')
    except OSError as error:
        add('cpu_model_error', str(error))
    try:
        first = cpu_sample()
        time.sleep(0.25)
        second = cpu_sample()
        total = second[0] - first[0]
        if total <= 0:
            raise ValueError('Sampel CPU belum tersedia.')
        add('cpu', round(max(0, min(100, 100 * (1 - (second[1] - first[1]) / total))), 1))
    except (OSError, ValueError, IndexError) as error:
        add('cpu_error', str(error))
    try:
        with open('/proc/meminfo') as source:
            memory = {line.split(':')[0]: int(line.split()[1]) * 1024 for line in source}
        add('memory_total', memory['MemTotal'])
        add('memory_used', memory['MemTotal'] - memory['MemAvailable'])
        add('swap_total', memory.get('SwapTotal', 0))
        add('swap_used', memory.get('SwapTotal', 0) - memory.get('SwapFree', 0))
    except (OSError, KeyError, ValueError) as error:
        add('memory_error', str(error))
    try:
        disk = shutil.disk_usage('/')
        add('disk_total', disk.total)
        add('disk_used', disk.used)
    except OSError as error:
        add('disk_error', str(error))
    try:
        seen = set()
        with open('/proc/mounts') as source:
            for line in source:
                fields = line.split()
                if len(fields) < 3:
                    continue
                device, mountpoint, fstype = (unescape_mount(fields[0]), unescape_mount(fields[1]), fields[2])
                if fstype in VIRTUAL_FSTYPES or not device.startswith('/') or device in seen:
                    continue
                try:
                    usage = shutil.disk_usage(mountpoint)
                except OSError:
                    continue
                if usage.total <= 0:
                    continue
                seen.add(device)
                rows.append(['disk', mountpoint, device, fstype, str(usage.used), str(usage.total)])
    except OSError as error:
        add('disk_list_error', str(error))
    try:
        # Reads whatever package list APT/DNF already has cached locally; it
        # does not refresh from the network (no implicit `apt-get update` /
        # `dnf makecache`), so this stays fast enough to run on every poll.
        count = None
        if shutil.which('apt-get') and shutil.which('dpkg-query'):
            # Matches `system_update.rs`'s actual command (plain `upgrade`,
            # not `dist-upgrade`): that one never installs/removes packages,
            # so it also never touches anything apt is holding back (new
            # dependencies, phased rollout). Counting with `dist-upgrade`
            # here would report packages Update System can never actually
            # resolve, so the number would never reach 0 no matter how many
            # times it's run.
            output = run(['apt-get', '-s', '-o', 'Debug::NoLocking=true', 'upgrade'])
            count = sum(1 for line in output.splitlines() if line.startswith('Inst '))
        elif shutil.which('dnf'):
            process = subprocess.run(['dnf', 'check-update', '--quiet'], stdout=subprocess.PIPE,
                                      stderr=subprocess.DEVNULL, timeout=20)
            if process.returncode == 0:
                count = 0
            elif process.returncode == 100:
                count = sum(1 for line in process.stdout.decode('utf-8', 'replace').splitlines() if line.strip())
            else:
                raise ValueError('dnf check-update gagal: exit ' + str(process.returncode))
        if count is None:
            add('updates_error', 'Package manager APT atau DNF tidak ditemukan di server.')
        else:
            add('updates_available', count)
    except subprocess.TimeoutExpired:
        add('updates_error', 'Pemeriksaan update melewati batas waktu.')
    except (OSError, ValueError) as error:
        add('updates_error', str(error))
    add('reboot_required', '1' if os.path.exists('/var/run/reboot-required') else '0')
    try:
        with open('/proc/uptime') as source:
            add('uptime', int(float(source.read().split()[0])))
    except (OSError, ValueError) as error:
        add('uptime_error', str(error))

def ports():
    try:
        output = run(['ss', '-H', '-lntu', '-p'])
        for line in output.splitlines():
            fields = line.split(None, 6)
            if len(fields) >= 6:
                rows.append(['socket', fields[0].upper(), fields[1], fields[4], fields[6] if len(fields) == 7 else 'Tidak tersedia / izin terbatas'])
    except (OSError, ValueError) as error:
        add('socket_error', str(error))
    found = False
    for name, command in [('ufw', ['ufw', 'status', 'numbered']),
                          ('firewalld', ['firewall-cmd', '--list-all'])]:
        if not shutil.which(command[0]):
            continue
        found = True
        try:
            output = run(root_command(command))
            rows.append(['firewall', name, output])
        except (OSError, ValueError) as error:
            rows.append(['firewall_error', name, str(error)])
    if not found:
        add('firewall_error', 'UFW atau firewalld tidak ditemukan; status akses tidak dapat disimpulkan dari socket.')

try:
    raw = sys.stdin.buffer.read(100000)
    lines = raw.decode('ascii').splitlines()
    kind = lines[0]
    fields = [bytes.fromhex(line).decode('utf-8') for line in lines[1:]]
    os.environ['LC_ALL'] = 'C'
    if kind == 'summary': summary()
    elif kind == 'ports': ports()
    else: raise ValueError('Jenis pembacaan tidak dikenal.')
    if len(rows) > 5000 or any(len(row) > 256 for row in rows):
        raise ValueError('Hasil melebihi batas tabel.')
    output = 'AWAN_LIVE_V1\n' + ''.join('\t'.join(str(cell).encode().hex() for cell in row) + '\n' for row in rows) + 'END\n'
    if len(output) > LIMIT:
        raise ValueError('Hasil melebihi 2 MiB. Persempit pilihan data.')
    sys.stdout.write(output)
except Exception as error:
    print('[Awan] ' + str(error), file=sys.stderr)
    sys.exit(1)
