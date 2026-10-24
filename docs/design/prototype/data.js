'use strict';
/* фейковые данные «сервера» */
function mkF(size, own = 'deploy', perm = 'rw-r--r--', mod = '24.10 11:03', text) { return { t: 'f', size, own, perm, mod, text }; }
function mkD(ch = {}, own = 'deploy', perm = 'rwxr-xr-x', mod = '12.10 09:14') { return { t: 'd', ch, own, perm, mod, size: 0 }; }
const FS_T = mkD({
  etc: mkD({
    nginx: mkD({ 'nginx.conf': mkF(2800, 'root', 'rw-r--r--', '12.10 09:14', 'user www-data;\nworker_processes auto;\npid /run/nginx.pid;\n\nevents {\n  worker_connections 1024;\n}\n\nhttp {\n  include /etc/nginx/mime.types;\n  sendfile on;\n  gzip on;\n  server {\n    listen 80;\n    server_name example.com;\n    root /var/www/html;\n    location / { try_files $uri $uri/ /index.php?$query_string; }\n  }\n}\n'), 'sites-enabled': mkD({ default: mkF(640, 'root') }, 'root') }, 'root'),
    ssh: mkD({ sshd_config: mkF(3200, 'root', 'rw-r--r--', '03.09 10:00', 'Port 22\nPermitRootLogin no\nPasswordAuthentication no\nPubkeyAuthentication yes\n') }, 'root'),
    hosts: mkF(220, 'root', 'rw-r--r--', '01.09 10:00', '127.0.0.1 localhost\n10.0.0.11 web-01\n'), hostname: mkF(7, 'root', 'rw-r--r--', '01.09 10:00', 'web-01\n'), 'os-release': mkF(380, 'root'),
  }, 'root'),
  var: mkD({
    www: mkD({ html: mkD({ assets: mkD({ 'app.css': mkF(88000), 'logo.svg': mkF(4200) }), uploads: mkD({ 'photo-1.jpg': mkF(2400000, 'www-data'), 'photo-2.jpg': mkF(1800000, 'www-data') }, 'www-data', 'rwxrwxr-x', '24.10 13:50'), 'index.php': mkF(4200, 'deploy', 'rw-r--r--', '24.10 11:03', '<?php\nrequire __DIR__.\'/vendor/autoload.php\';\n$app = require_once __DIR__.\'/bootstrap/app.php\';\n$app->run();\n'), '.env': mkF(1100, 'deploy', 'rw-------', '03.10 18:22', 'APP_ENV=production\nAPP_DEBUG=false\nDB_HOST=10.0.0.20\nDB_PASSWORD=********\n'), 'app.js': mkF(318000, 'deploy', 'rw-r--r--', '22.10 16:40', 'console.log("hello");\n'), 'backup.tar.gz': mkF(1400000000, 'root', 'rw-r-----', '23.10 03:00'), 'robots.txt': mkF(86, 'deploy', 'rw-r--r--', '01.09 10:00', 'User-agent: *\nDisallow:\n') }) }, 'www-data'),
    log: mkD({ nginx: mkD({ 'access.log': mkF(184000000, 'www-data', 'rw-r-----', '24.10 14:02'), 'error.log': mkF(2100000, 'www-data', 'rw-r-----', '24.10 14:01') }, 'root'), syslog: mkF(31000000, 'root', 'rw-r-----', '24.10 14:02'), 'auth.log': mkF(1200000, 'root', 'rw-r-----', '24.10 14:00') }, 'root'),
    lib: mkD({ docker: mkD({}, 'root'), postgresql: mkD({}, 'postgres') }, 'root'),
  }, 'root'),
  home: mkD({ deploy: mkD({ '.bashrc': mkF(3800, 'deploy', 'rw-r--r--', '01.09 10:00', '# ~/.bashrc\nalias ll="ls -la"\n'), 'notes.txt': mkF(420, 'deploy', 'rw-r--r--', '20.10 12:00', 'TODO:\n - обновить nginx\n - проверить бэкапы\n'), releases: mkD({ 'release-142.zip': mkF(12000000) }) }) }, 'root'),
  srv: mkD({ 'backup.sh': mkF(900, 'root', 'rwxr-xr-x', '15.09 08:00', '#!/bin/bash\ntar czf /var/backups/site-$(date +%F).tgz /var/www/html\n'), app: mkD({}) }, 'root'),
  usr: mkD({ bin: mkD({}, 'root'), lib: mkD({}, 'root') }, 'root'), tmp: mkD({}, 'root', 'rwxrwxrwt'),
}, 'root');
function fsGet(root, path) { let n = root; for (const p of path.split('/').filter(Boolean)) { if (!n || n.t !== 'd') return null; n = n.ch[p]; } return n || null; }
function fsNorm(cwd, p) { const parts = (p.startsWith('/') ? [] : cwd.split('/').filter(Boolean)); for (const s of p.split('/')) { if (!s || s === '.') continue; if (s === '..') parts.pop(); else if (s === '~') { parts.length = 0; parts.push('home', 'deploy'); } else parts.push(s); } return '/' + parts.join('/'); }
const FILE_INDEX = ['/etc/nginx/nginx.conf', '/etc/ssh/sshd_config', '/var/www/html/index.php', '/var/www/html/.env', '/var/log/nginx/error.log', '/srv/backup.sh', '/home/deploy/notes.txt'];
const SERVICES = [
  { name: 'nginx', desc: 'A high performance web server', state: 'active', en: true, mem: '8.4 МБ', pid: 812 },
  { name: 'postgresql', desc: 'PostgreSQL RDBMS', state: 'active', en: true, mem: '1.2 ГБ', pid: 1204 },
  { name: 'redis', desc: 'Advanced key-value store', state: 'failed', en: true, mem: '—', pid: 0 },
  { name: 'ssh', desc: 'OpenBSD Secure Shell server', state: 'active', en: true, mem: '5.1 МБ', pid: 640, critical: true },
  { name: 'cron', desc: 'Regular background program processing', state: 'active', en: true, mem: '1.2 МБ', pid: 601 },
  { name: 'docker', desc: 'Docker Application Container Engine', state: 'active', en: true, mem: '120 МБ', pid: 905 },
  { name: 'fail2ban', desc: 'Ban hosts that cause multiple auth errors', state: 'active', en: true, mem: '22 МБ', pid: 720 },
  { name: 'ufw', desc: 'Uncomplicated firewall', state: 'active', en: true, mem: '—', pid: 0 },
  { name: 'systemd-networkd', desc: 'Network Configuration', state: 'active', en: true, mem: '3.0 МБ', pid: 540, critical: true },
  { name: 'cups', desc: 'CUPS Scheduler', state: 'inactive', en: false, mem: '—', pid: 0 },
  { name: 'unattended-upgrades', desc: 'Unattended Upgrades Shutdown', state: 'active', en: true, mem: '9 МБ', pid: 700 },
];
const PACKAGES = [
  { n: 'nginx', v: '1.24.0-2', nv: '1.24.0-2ubuntu7.1', sec: true, inst: true, size: '1.2 МБ' },
  { n: 'openssl', v: '3.0.13', nv: '3.0.13-0ubuntu3.4', sec: true, inst: true, size: '2.0 МБ' },
  { n: 'libc6', v: '2.39-0ubuntu8.2', nv: '2.39-0ubuntu8.3', sec: true, inst: true, size: '3.1 МБ' },
  { n: 'curl', v: '8.5.0-2', nv: '8.5.0-2ubuntu10.5', sec: false, inst: true, size: '240 КБ' },
  { n: 'postgresql-16', v: '16.3-1', nv: '16.4-1', sec: false, inst: true, size: '14 МБ' },
  { n: 'htop', v: '3.3.0-4', sec: false, inst: true }, { n: 'git', v: '2.43.0', sec: false, inst: true }, { n: 'vim', v: '9.1.0016', sec: false, inst: true },
  { n: 'docker-ce', v: '26.1.0', sec: false, inst: true }, { n: 'ufw', v: '0.36.2', sec: false, inst: true }, { n: 'fail2ban', v: '1.0.2', sec: false, inst: true },
  { n: 'nginx-extras', v: '1.24.0', sec: false, inst: false }, { n: 'ncdu', v: '1.19', sec: false, inst: false }, { n: 'tmux', v: '3.4', sec: false, inst: false }, { n: 'nodejs', v: '20.12', sec: false, inst: false },
];
const PROCS = () => [
  { pid: 1, name: 'systemd', user: 'root', cpu: 0.0, mem: 12e6, st: 'S', cmd: '/sbin/init' },
  { pid: 601, name: 'cron', user: 'root', cpu: 0.0, mem: 1.2e6, st: 'S', cmd: '/usr/sbin/cron -f' },
  { pid: 640, name: 'sshd', user: 'root', cpu: 0.1, mem: 5.1e6, st: 'S', cmd: 'sshd: /usr/sbin/sshd -D' },
  { pid: 720, name: 'fail2ban', user: 'root', cpu: 0.3, mem: 22e6, st: 'S', cmd: '/usr/bin/python3 /usr/bin/fail2ban-server' },
  { pid: 812, name: 'nginx', user: 'www-data', cpu: 2.1, mem: 8.4e6, st: 'S', cmd: 'nginx: worker process' },
  { pid: 813, name: 'nginx', user: 'www-data', cpu: 1.8, mem: 8.1e6, st: 'S', cmd: 'nginx: worker process' },
  { pid: 905, name: 'dockerd', user: 'root', cpu: 1.4, mem: 120e6, st: 'S', cmd: '/usr/bin/dockerd -H fd://' },
  { pid: 1204, name: 'postgres', user: 'postgres', cpu: 12.7, mem: 1.2e9, st: 'S', cmd: 'postgres: 16/main: checkpointer' },
  { pid: 1388, name: 'php-fpm', user: 'www-data', cpu: 4.2, mem: 64e6, st: 'S', cmd: 'php-fpm: pool www' },
  { pid: 2210, name: 'node', user: 'deploy', cpu: 7.4, mem: 612e6, st: 'R', cmd: 'node /srv/app/server.js' },
  { pid: 2415, name: 'redis-server', user: 'redis', cpu: 0.6, mem: 42e6, st: 'S', cmd: '/usr/bin/redis-server *:6379' },
  { pid: 3391, name: 'java', user: 'deploy', cpu: 18.3, mem: 1.6e9, st: 'S', cmd: 'java -jar worker.jar' },
  { pid: 4410, name: 'sshd', user: 'deploy', cpu: 0.0, mem: 7e6, st: 'S', cmd: 'sshd: deploy@pts/0' },
  { pid: 4411, name: 'bash', user: 'deploy', cpu: 0.0, mem: 4e6, st: 'S', cmd: '-bash' },
  { pid: 5120, name: 'rsyslogd', user: 'syslog', cpu: 0.1, mem: 6e6, st: 'S', cmd: '/usr/sbin/rsyslogd -n' },
  { pid: 5301, name: 'python3', user: 'deploy', cpu: 3.3, mem: 88e6, st: 'S', cmd: 'python3 /srv/app/worker.py' },
];
/* дерево для анализа диска (байты) */
function dnode(n, ch, kind) { return { n, ch, kind, s: ch ? ch.reduce((a, c) => a + c.s, 0) : 0 }; }
function lnode(n, s, kind) { return { n, s, kind }; }
const GB = 1e9, MB = 1e6;
const DISK_TREE = () => dnode('/', [
  dnode('var', [
    dnode('lib', [dnode('docker', [lnode('overlay2', 14.2 * GB, 'img'), lnode('volumes', 5.1 * GB, 'db'), lnode('containers', 2.1 * GB, 'log')], 'img'), dnode('postgresql', [lnode('16/main', 6.8 * GB, 'db'), lnode('wal', 1.6 * GB, 'db')], 'db'), lnode('apt', 0.4 * GB, 'cache')]),
    dnode('log', [lnode('nginx/access.log', 4.2 * GB, 'log'), lnode('journal', 3.1 * GB, 'log'), lnode('syslog', 0.9 * GB, 'log'), lnode('nginx/error.log', 0.4 * GB, 'log')], 'log'),
    dnode('www', [lnode('html/uploads', 3.4 * GB, 'media'), lnode('html/backup.tar.gz', 1.4 * GB, 'arc'), lnode('html/vendor', 0.3 * GB, 'code')]),
    dnode('cache', [lnode('apt/archives', 1.9 * GB, 'cache'), lnode('nginx', 0.5 * GB, 'cache')], 'cache'),
  ]),
  dnode('home', [dnode('deploy', [lnode('releases', 4.6 * GB, 'arc'), lnode('.cache', 1.1 * GB, 'cache'), lnode('node_modules', 0.8 * GB, 'code')])]),
  dnode('usr', [lnode('lib', 3.2 * GB, 'code'), lnode('share', 1.4 * GB, 'code'), lnode('bin', 0.5 * GB, 'code')]),
  dnode('srv', [lnode('app', 1.8 * GB, 'code'), lnode('backups', 2.6 * GB, 'arc')]),
  dnode('opt', [lnode('grafana', 0.9 * GB, 'code')]),
]);
const KIND_COLOR = { img: '#4da3ff', db: '#b388ff', log: '#ffc53d', media: '#3dff8a', arc: '#ff8a4d', cache: '#8bd3ff', code: '#00e5ff' };
const KIND_NAME = { img: 'образы', db: 'БД', log: 'логи', media: 'медиа', arc: 'архивы', cache: 'кэш', code: 'код/система' };
