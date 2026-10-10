import os
import sys
import stat
import datetime

def hextext(value):
    return value.encode('utf-8').hex()

try:
    requested, query, hidden = sys.argv[1:]
    home = os.path.realpath(os.path.expanduser('~'))
    path = os.path.realpath(os.path.expanduser(requested))
    records = []
    truncated = False
    skipped = 0
    size_used = 0
    with os.scandir(path) as directory:
        for entry in directory:
            try:
                name_hex = hextext(entry.name)
            except UnicodeError:
                skipped += 1
                continue
            if (hidden != '1' and entry.name.startswith('.')) or query.casefold() not in entry.name.casefold():
                continue
            if len(records) >= 2000:
                truncated = True
                break
            kind, size, modified, permissions, target = 'unknown', '-', 'Tidak tersedia', '?', ''
            try:
                info = entry.stat(follow_symlinks=False)
                size = str(info.st_size)
                permissions = stat.filemode(info.st_mode)
                if stat.S_ISLNK(info.st_mode):
                    kind = 'link'
                    target = os.readlink(entry.path)
                    if entry.is_dir(follow_symlinks=True):
                        kind = 'dirlink'
                elif stat.S_ISDIR(info.st_mode):
                    kind = 'dir'
                elif stat.S_ISREG(info.st_mode):
                    kind = 'file'
                else:
                    kind = 'other'
                try:
                    modified = datetime.datetime.fromtimestamp(info.st_mtime, datetime.timezone.utc).strftime('%Y-%m-%d %H:%M UTC')
                except (ValueError, OverflowError, OSError):
                    pass
            except OSError:
                pass
            # Non-UTF-8 symlink targets are display-only; never used for navigation.
            target = target.encode('utf-8', 'replace').decode('utf-8')
            # Whether this folder is a Git repository root (`.git` may be a
            # directory, or a file for worktrees/submodules). Checked here
            # because `.git` itself is hidden from the listing by default.
            git = '0'
            if kind in ('dir', 'dirlink'):
                try:
                    git = '1' if os.path.lexists(os.path.join(entry.path, '.git')) else '0'
                except OSError:
                    pass
            line = '\t'.join([kind, size, hextext(modified), hextext(permissions), name_hex, hextext(target), git]) + '\n'
            if size_used + len(line) > 1000000:
                truncated = True
                break
            size_used += len(line)
            records.append((kind not in ('dir', 'dirlink'), entry.name.casefold(), entry.name, line))
    records.sort(key=lambda row: row[:3])
    sys.stdout.write('AWAN_DIR_V1\t' + hextext(path) + '\t' + hextext(home) + '\t' + str(int(truncated)) + '\t' + str(skipped) + '\n')
    for record in records:
        sys.stdout.write(record[3])
    sys.stdout.write('END\n')
except Exception as error:
    print('[Awan] ' + str(error), file=sys.stderr)
    sys.exit(1)
