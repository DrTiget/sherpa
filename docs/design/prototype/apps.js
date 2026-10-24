'use strict';
/* Утилиты: терминал, файлы, редактор, монитор, диски */
function reg(def) { APPS[def.id] = def; }
function tabsBar(items, cur, fn) { const seg = h('div.seg'); const render = () => seg.replaceChildren(...items.map(([k, t]) => h('button' + (k === cur.v ? '.on' : ''), { onclick: () => { cur.v = k; render(); fn(k); } }, t))); render(); seg.set = k => { cur.v = k; render(); fn(k); }; return seg; }
function fitCanvas(c) { const r = c.getBoundingClientRect(), d = window.devicePixelRatio || 1; const w = Math.max(10, Math.round(r.width)), hh = +c.dataset.h || 80; if (c.width !== w * d || c.height !== hh * d) { c.width = w * d; c.height = hh * d; c.style.height = hh + 'px'; } const g = c.getContext('2d'); g.setTransform(d, 0, 0, d, 0, 0); return { g, w, h: hh }; }
function drawChart(c, series, max, grid = true) {
  const { g, w, h: hh } = fitCanvas(c); g.clearRect(0, 0, w, hh);
  if (grid) { g.strokeStyle = 'rgba(255,255,255,.07)'; g.lineWidth = 1; for (let i = 1; i < 4; i++) { const y = Math.round(hh * i / 4) + .5; g.beginPath(); g.moveTo(0, y); g.lineTo(w, y); g.stroke(); } }
  series.forEach(({ d, color }) => { const n = d.length, xs = i => i * w / (n - 1), ys = v => hh - 3 - clamp(v / max, 0, 1) * (hh - 8);
    g.beginPath(); d.forEach((v, i) => i ? g.lineTo(xs(i), ys(v)) : g.moveTo(xs(i), ys(v))); g.strokeStyle = color; g.lineWidth = 1.6; g.lineJoin = 'round'; g.shadowColor = color; g.shadowBlur = 6; g.stroke(); g.shadowBlur = 0;
    g.lineTo(w, hh); g.lineTo(0, hh); g.closePath(); const gr = g.createLinearGradient(0, 0, 0, hh); gr.addColorStop(0, color + '55'); gr.addColorStop(1, color + '00'); g.fillStyle = gr; g.fill(); });
}
const kindOf = n => n.kind || (n.ch && kindOf(n.ch.slice().sort((a, b) => b.s - a.s)[0])) || 'code';
const col = v => v > 80 ? 'var(--rd)' : v > 55 ? 'var(--am)' : 'var(--gr)';
function permStr(p) { return p; }

/* ============ ТЕРМИНАЛ ============ */
reg({ id: 'terminal', name: 'Терминал', ico: '>_', c: '#3dff8a', w: 760, h: 460, cat: 'Разработка', desc: 'SSH-сессия (PTY)', kw: 'shell bash ssh консоль',
  build(win, desk, opts) {
    const s = desk.srv, prompt = (cwd) => { const dir = cwd === '/home/deploy' ? '~' : cwd; return [h('div', h('span.b', '┌──('), h('span.c', s.user), h('span.b', '@'), h('span.c', s.name), h('span.b', ')-['), h('span.w', dir), h('span.b', ']')), h('div', h('span.b', '└─'), h('span.c', s.user === 'root' ? '# ' : '$ '))]; };
    desk.fs = desk.fs || structuredClone(FS_T);
    const tabsEl = h('div.ttabs'); const wrap = h('div', { style: 'flex:1;min-height:0;display:flex;flex-direction:column' });
    win.body.append(tabsEl, wrap, h('div.wstat', dot(desk.state === 'lost' ? 'err' : 'on'), h('span', desk.state === 'lost' ? 'сессия отключена' : 'connected · ' + s.lat + ' ms'), h('span.cy', s.user + '@' + s.host + ':' + s.port), h('span.sp'), h('span', '120×32 · UTF-8')));
    const sessions = []; let cur = null, n = 0;
    function newSession(cmd) {
      const se = { name: 'bash ' + (++n), cwd: '/home/deploy', hist: [], hi: 0, el: h('div.term') }; sessions.push(se);
      const out = (html, cls) => { const d = h('div', { html: html }); if (cls) d.className = cls; se.el.insertBefore(d, se.pl); se.el.scrollTop = 1e9; return d; };
      se.out = out;
      se.pl = h('div'); se.el.append(se.pl);
      se.input = h('input', { spellcheck: false, autocomplete: 'off', onkeydown: e => {
        if (e.key === 'Enter') { const v = se.input.value; se.input.value = ''; se.hist.push(v); se.hi = se.hist.length; run(se, v); }
        else if (e.key === 'ArrowUp') { se.hi = Math.max(0, se.hi - 1); se.input.value = se.hist[se.hi] || ''; e.preventDefault(); }
        else if (e.key === 'ArrowDown') { se.hi = Math.min(se.hist.length, se.hi + 1); se.input.value = se.hist[se.hi] || ''; e.preventDefault(); }
        else if (e.key === 'l' && e.ctrlKey) { e.preventDefault(); clear(se); }
        else if (e.key === 'c' && e.ctrlKey) { out(`<span class="b">└─</span><span class="c">$ </span>${esc(se.input.value)}^C`); se.input.value = ''; showPrompt(se); }
        else if (e.key === 'Tab') { e.preventDefault(); complete(se); } } });
      se.el.addEventListener('click', () => { if (!getSelection().toString()) se.input.focus(); });
      showPrompt(se, true);
      if (sessions.length === 1) { out('<span class="d">Linux ' + esc(s.name) + ' 6.8.0-45-generic · ' + esc(s.os) + '</span>'); out('<span class="d">Последний вход: Пт 24 окт 09:14:02 с 10.0.0.2. Введите <b>help</b>.</span>'); }
      switchTo(se); if (cmd) setTimeout(() => { se.input.value = cmd; se.input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' })); }, 60);
      return se;
    }
    function showPrompt(se, first) { se.pl.replaceChildren(...prompt(se.cwd)); const last = se.pl.lastChild; last.append(se.input); se.el.scrollTop = 1e9; setTimeout(() => se.input.focus(), 0); }
    function clear(se) { [...se.el.children].forEach(c => { if (c !== se.pl) c.remove(); }); }
    function renderTabs() { tabsEl.replaceChildren(...sessions.map(se => h('div.ttab' + (se === cur ? '.on' : ''), { onclick: () => switchTo(se) }, se.name, sessions.length > 1 ? h('span.sub', { style: 'margin-left:8px', onclick: e => { e.stopPropagation(); closeS(se); } }, '×') : null)), h('div.ttab', { style: 'color:var(--cy)', onclick: () => newSession() }, '+')); }
    function switchTo(se) { cur = se; wrap.replaceChildren(se.el); renderTabs(); setTimeout(() => se.input.focus(), 0); }
    function closeS(se) { sessions.splice(sessions.indexOf(se), 1); if (!sessions.length) return win.close(); switchTo(sessions[0]); }
    function complete(se) { const v = se.input.value, parts = v.split(' '), last = parts.pop(); const dir = last.includes('/') ? fsNorm(se.cwd, last.slice(0, last.lastIndexOf('/') + 1)) : se.cwd; const nd = fsGet(desk.fs, dir); if (!nd || nd.t !== 'd') return; const pre = last.slice(last.lastIndexOf('/') + 1); const m = Object.keys(nd.ch).filter(k => k.startsWith(pre)); if (m.length === 1) se.input.value = [...parts, last.slice(0, last.lastIndexOf('/') + 1) + m[0] + (nd.ch[m[0]].t === 'd' ? '/' : '')].join(' '); else if (m.length) se.out('<span class="d">' + m.join('  ') + '</span>'); }
    const svcS = n => SERVICES.find(x => x.name === n);
    function run(se, line) {
      const out = se.out; out(`<span class="b">└─</span><span class="c">${s.user === 'root' ? '# ' : '$ '}</span>${esc(line)}`);
      const [cmd, ...args] = line.trim().split(/\s+/).filter(Boolean); const a = args.filter(x => !x.startsWith('-')), fl = args.filter(x => x.startsWith('-')).join('');
      const P = t => out(esc(t)), D = t => out('<span class="d">' + esc(t) + '</span>');
      if (!cmd) return showPrompt(se);
      const C = {
        help: () => P('Доступны: help ls cd pwd cat echo whoami hostname uname uptime date free df ps top ip ss systemctl docker mkdir touch rm neofetch history clear exit sudo'),
        pwd: () => P(se.cwd), whoami: () => P(s.user), hostname: () => P(s.name), date: () => P(new Date().toString()), echo: () => P(args.join(' ')),
        uname: () => P('Linux ' + s.name + ' 6.8.0-45-generic #45-Ubuntu SMP x86_64 GNU/Linux'),
        uptime: () => P(' ' + hhmmss() + ' up 41 days,  3:12,  1 user,  load average: ' + desk.m.load.map(x => x.toFixed(2)).join(', ')),
        free: () => { const t = s.ram * 1024, u = Math.round(t * desk.m.ram[59] / 100); P('              total        used        free      shared  buff/cache   available\nMem:        ' + String(t).padStart(8) + 'Mi ' + String(u).padStart(9) + 'Mi ' + String(t - u).padStart(9) + 'Mi      12Mi      1.2Gi     ' + String(t - u).padStart(5) + 'Mi\nSwap:          2048Mi       102Mi      1946Mi'); },
        df: () => out('<span class="d">Filesystem      Size  Used Avail Use% Mounted on</span>\n/dev/vda1        80G   52G   25G <span class="y">68%</span> /\n/dev/vdb1       500G  465G   35G <span class="r">93%</span> /data\ntmpfs           3.9G     0  3.9G   0% /dev/shm'),
        ps: () => out('<span class="d">  PID TTY          TIME CMD</span>\n' + PROCS().filter(p => p.user === s.user || p.pid < 700).slice(0, 7).map(p => String(p.pid).padStart(5) + ' pts/0    00:00:0' + (p.pid % 9) + ' ' + p.name).join('\n')),
        top: () => out('<span class="d">top - ' + hhmmss() + ' up 41 days, load average: ' + desk.m.load[0].toFixed(2) + '\n  PID USER      %CPU %MEM COMMAND</span>\n' + PROCS().sort((x, y) => y.cpu - x.cpu).slice(0, 6).map(p => String(p.pid).padStart(5) + ' ' + p.user.padEnd(9) + String(p.cpu.toFixed(1)).padStart(5) + String((p.mem / s.ram / 1e7).toFixed(1)).padStart(5) + ' ' + p.name).join('\n')),
        ip: () => out(`1: <span class="g">lo</span>: &lt;LOOPBACK,UP&gt; inet 127.0.0.1/8\n2: <span class="g">eth0</span>: &lt;BROADCAST,MULTICAST,UP&gt; mtu 1500\n    link/ether 52:54:00:12:34:56\n    inet <span class="c">${esc(s.host.match(/^[\d.]+$/) ? s.host : '203.0.113.7')}/24</span> brd 10.0.0.255 scope global eth0`),
        ss: () => out('<span class="d">Netid State  Local Address:Port  Process</span>\ntcp   LISTEN 0.0.0.0:22         sshd\ntcp   LISTEN 0.0.0.0:80         nginx\ntcp   LISTEN 127.0.0.1:5432     postgres\ntcp   LISTEN 127.0.0.1:6379     redis-server'),
        neofetch: () => out('<span class="c">       .--.      </span><span class="w">' + esc(s.user + '@' + s.name) + '</span>\n<span class="c">      |o_o |     </span>OS: ' + esc(s.os) + '\n<span class="c">      |:_/ |     </span>Kernel: 6.8.0-45-generic\n<span class="c">     //   \\ \\    </span>Uptime: 41 days\n<span class="c">    (|     | )   </span>CPU: ' + s.cores + ' cores\n<span class="c">   /\'\\_   _/`\\  </span>Memory: ' + s.ram + ' GiB'),
        history: () => P(se.hist.map((x, i) => String(i + 1).padStart(4) + '  ' + x).join('\n')),
        clear: () => clear(se),
        exit: () => { out('<span class="d">logout\nСессия завершена.</span>'); const b = h('button.btn.sm', { onclick: () => { b.parentNode.remove(); showPrompt(se); } }, 'Переподключить'); se.el.insertBefore(h('div', b), se.pl); se.pl.replaceChildren(); return 1; },
        cd: () => { const p = fsNorm(se.cwd, a[0] || '~'), nd = fsGet(desk.fs, p); if (!nd) P('bash: cd: ' + a[0] + ': Нет такого файла или каталога'); else if (nd.t !== 'd') P('bash: cd: ' + a[0] + ': Не каталог'); else se.cwd = p; },
        ls: () => { const p = fsNorm(se.cwd, a[0] || '.'), nd = fsGet(desk.fs, p); if (!nd) return P('ls: невозможно получить доступ к «' + a[0] + '»'); if (nd.t === 'f') return P(a[0]);
          let ks = Object.keys(nd.ch).filter(k => fl.includes('a') || !k.startsWith('.')).sort(); if (fl.includes('l')) out('<span class="d">итого ' + ks.length + '</span>\n' + ks.map(k => { const c = nd.ch[k]; return (c.t === 'd' ? 'd' : '-') + c.perm + ' ' + c.own.padEnd(9) + String(c.size || 4096).padStart(10) + ' ' + c.mod + ' ' + (c.t === 'd' ? '<span class="b"><b>' + esc(k) + '</b></span>' : esc(k)); }).join('\n')); else out(ks.map(k => nd.ch[k].t === 'd' ? '<span class="b"><b>' + esc(k) + '</b></span>' : esc(k)).join('  ')); },
        cat: () => { const nd = fsGet(desk.fs, fsNorm(se.cwd, a[0] || '')); if (!nd) P('cat: ' + a[0] + ': Нет такого файла или каталога'); else if (nd.t === 'd') P('cat: ' + a[0] + ': Это каталог'); else P(nd.text != null ? nd.text : '[бинарные данные, ' + fmtB(nd.size) + ']'); },
        mkdir: () => { const p = fsNorm(se.cwd, a[0] || ''), par = fsGet(desk.fs, p.slice(0, p.lastIndexOf('/')) || '/'); if (par && par.t === 'd' && a[0]) par.ch[p.split('/').pop()] = mkD({}, s.user, 'rwxr-xr-x', 'сейчас'); else P('mkdir: не удалось создать каталог'); },
        touch: () => { const p = fsNorm(se.cwd, a[0] || ''), par = fsGet(desk.fs, p.slice(0, p.lastIndexOf('/')) || '/'); if (par && par.t === 'd' && a[0]) par.ch[p.split('/').pop()] = mkF(0, s.user, 'rw-r--r--', 'сейчас', ''); },
        rm: () => { const p = fsNorm(se.cwd, a[0] || ''), par = fsGet(desk.fs, p.slice(0, p.lastIndexOf('/')) || '/'), k = p.split('/').pop(); if (par && par.ch && par.ch[k]) { if (par.ch[k].own === 'root' && s.user !== 'root') P('rm: невозможно удалить «' + a[0] + '»: Отказано в доступе'); else delete par.ch[k]; } else P('rm: нет такого файла'); },
        systemctl: () => { const act = a[0], nm = (a[1] || '').replace('.service', ''), sv = svcS(nm); if (!sv) return P('Unit ' + nm + '.service could not be found.');
          if (act === 'status') out(`<span class="${sv.state === 'active' ? 'g' : 'r'}">●</span> <b>${esc(sv.name)}.service</b> - ${esc(sv.desc)}\n     Loaded: loaded (/lib/systemd/system/${esc(sv.name)}.service; ${sv.en ? 'enabled' : 'disabled'})\n     Active: <span class="${sv.state === 'active' ? 'g' : 'r'}"><b>${sv.state === 'active' ? 'active (running)' : sv.state === 'failed' ? 'failed (Result: exit-code)' : 'inactive (dead)'}</b></span>${sv.pid ? '\n   Main PID: ' + sv.pid : ''}`);
          else if (['restart', 'start', 'stop'].includes(act)) { if (s.user !== 'root' && !line.startsWith('sudo')) return P('Failed to ' + act + ' ' + nm + '.service: Interactive authentication required. Используйте sudo.'); sv.state = act === 'stop' ? 'inactive' : 'active'; desk.emit('svc'); } else P('Unknown command verb ' + act); },
        docker: () => { if (a[0] === 'ps') out('<span class="d">CONTAINER ID  IMAGE           STATUS        PORTS                NAMES</span>\na1b2c3d4e5f6  myapp:1.4       Up 3 days     0.0.0.0:8080->80     app\n9f8e7d6c5b4a  postgres:16     Up 3 days     5432/tcp             db\nd4c3b2a19087  myapp:1.4       <span class="r">Exited (1)</span>   —                    worker'); else P('docker: используйте «docker ps»'); },
        sudo: () => { if (s.user === 'root') return run(se, args.join(' ')); const cmd2 = args.join(' '); out('<span class="d">[sudo] пароль для ' + esc(s.user) + ': </span>********'); setTimeout(() => { desk.log('sudo ' + cmd2); const saved = s.user; s.user = 'root'; try { run(se, cmd2.replace(/^/, '')); } finally { s.user = saved; } }, 250); return 1; },
      };
      const fn = C[cmd]; let skip;
      if (fn) { try { skip = fn(); } catch (e) { out('<span class="r">' + esc(e.message) + '</span>'); } } else out(`bash: ${esc(cmd)}: команда не найдена`);
      if (skip !== 1) showPrompt(se);
    }
    newSession(opts.cmd);
    win.cleanup.push(desk.on(ev => { if (ev === 'state') { const st = $('.wstat', win.body); if (st) { st.firstChild.replaceWith(dot(desk.state === 'lost' ? 'err' : 'on')); st.children[1].textContent = desk.state === 'lost' ? 'сессия отключена' : 'connected · ' + s.lat + ' ms'; } } }));
    win.body.addEventListener('wfocus', () => cur && cur.input.focus());
    return { run: c => { cur.input.value = c; cur.input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' })); } };
  } });

/* ============ ФАЙЛЫ ============ */
const sysPath = p => /^\/(etc|boot|usr|bin|sbin|lib|var\/lib)(\/|$)/.test(p);
reg({ id: 'files', name: 'Файлы', ico: 'Fl', c: '#367bf0', w: 960, h: 560, cat: 'Файлы', desc: 'SFTP: просмотр, права, передача', kw: 'sftp files файловый менеджер',
  build(win, desk, opts) {
    desk.fs = desk.fs || structuredClone(FS_T); const fs = () => desk.fs, s = desk.srv;
    let path = opts.path || '/var/www/html', sel = null, view = 'list', hid = false, sortK = 'name', sortD = 1;
    const tree = h('div.ftree'), list = h('div.flist'), props = h('div.fprops'), crumbs = h('div.row.mono.grow', { style: 'background:rgba(var(--pan),.5);border:1px solid var(--line);border-radius:6px;padding:5px 10px;gap:4px;overflow:hidden' }), xf = h('div.xfers'), st = h('div.wstat');
    const search = h('input.inp', { placeholder: 'Поиск', style: 'width:140px', oninput: () => render() });
    const nodeAt = p => fsGet(fs(), p);
    const bar = h('div.tbar', h('button.btn.sm', { onclick: () => cd(path.slice(0, path.lastIndexOf('/')) || '/') }, '↑'), crumbs,
      h('div.seg', ...[['list', 'Список'], ['icons', 'Иконки']].map(([k, t]) => h('button' + (k === view ? '.on' : ''), { onclick: e => { view = k; $$('.seg button', bar).forEach(b => b.classList.toggle('on', b === e.currentTarget)); render(); } }, t))), search,
      h('button.btn.sm', { onclick: () => mk('d') }, '+ Папка'), h('button.btn.sm', { onclick: () => mk('f') }, '+ Файл'), h('button.btn.sm.p', { onclick: () => upload([['новый-файл.zip', 3.2e6]]) }, '↑ Загрузить'), h('button.btn.sm', { title: 'Скрытые файлы', onclick: e => { hid = !hid; e.currentTarget.classList.toggle('p', hid); render(); } }, '.*'));
    win.body.append(bar, h('div.files', tree, list, props), xf, st);
    function cd(p) { if (!nodeAt(p) || nodeAt(p).t !== 'd') return toast('Нет такого каталога', 1); path = p; sel = null; render(); win.setTitle('Файлы', '· ' + s.name + ':' + p); }
    function items() { const nd = nodeAt(path); let ks = Object.keys(nd.ch).filter(k => (hid || !k.startsWith('.')) && k.toLowerCase().includes(search.value.toLowerCase())); ks.sort((a, b) => { const A = nd.ch[a], B = nd.ch[b]; if (A.t !== B.t) return A.t === 'd' ? -1 : 1; const v = sortK === 'size' ? A.size - B.size : sortK === 'mod' ? A.mod.localeCompare(B.mod) : sortK === 'own' ? A.own.localeCompare(B.own) : a.localeCompare(b); return v * sortD; }); return ks.map(k => [k, nd.ch[k]]); }
    function treeRows(p, depth, out) { const nd = nodeAt(p); Object.keys(nd.ch).filter(k => nd.ch[k].t === 'd').sort().forEach(k => { const full = (p === '/' ? '' : p) + '/' + k, open = path === full || path.startsWith(full + '/'); out.push(h('div' + (path === full ? '.on' : ''), { style: `padding-left:${8 + depth * 12}px`, onclick: () => cd(full) }, (open ? '▾ ' : '▸ ') + k)); if (open) treeRows(full, depth + 1, out); }); return out; }
    function render() {
      const nd = nodeAt(path); if (!nd) return cd('/');
      const segs = path.split('/').filter(Boolean); crumbs.replaceChildren(h('span', { style: 'cursor:pointer', onclick: () => cd('/') }, '/'), ...segs.flatMap((sg, i) => [h('span.sub', '›'), h('span' + (i === segs.length - 1 ? '.cy' : ''), { style: 'cursor:pointer', onclick: () => cd('/' + segs.slice(0, i + 1).join('/')) }, sg)]));
      tree.replaceChildren(h('div.cap', { style: 'padding:4px 8px' }, 'Избранное'), ...['/home/deploy', '/etc', '/var/log', '/srv'].map(f => h('div' + (path === f ? '.on' : ''), { onclick: () => cd(f) }, f)), h('div.cap', { style: 'padding:8px 8px 4px' }, 'Дерево'), h('div' + (path === '/' ? '.on' : ''), { onclick: () => cd('/') }, '▾ /'), ...treeRows('/', 1, []));
      const its = items();
      if (view === 'list') {
        const th = (k, t, w) => h('th', { style: `width:${w}px;cursor:pointer`, onclick: () => { sortD = sortK === k ? -sortD : 1; sortK = k; render(); } }, t + (sortK === k ? (sortD > 0 ? ' ▾' : ' ▴') : ''));
        list.replaceChildren(h('table.t.mono', h('thead', h('tr', th('name', 'Имя'), th('size', 'Размер', 90), th('mod', 'Изменён', 100), th('own', 'Владелец', 90), h('th', { style: 'width:100px' }, 'Права'))),
          h('tbody', its.length ? its.map(([k, n]) => h('tr' + (sel === k ? '.sel' : ''), { onclick: () => { sel = k; render(); }, ondblclick: () => openIt(k), oncontextmenu: e => { e.preventDefault(); sel = k; render(); ctxMenu(e, k); } },
            h('td', h('span.fi' + (n.t === 'd' ? '.dir' : '')), k), h('td.sub', n.t === 'd' ? '—' : fmtB(n.size)), h('td.sub', n.mod), h('td' + (n.own === 'root' ? '.rd' : '.sub'), n.own), h('td.sub', n.perm))) : h('tr', h('td', { colSpan: 5, style: 'text-align:center;padding:30px;color:var(--sub)' }, 'Каталог пуст')))));
      } else list.replaceChildren(h('div.icons-view', its.map(([k, n]) => h('div.fic' + (sel === k ? '.sel' : ''), { onclick: () => { sel = k; render(); }, ondblclick: () => openIt(k), oncontextmenu: e => { e.preventDefault(); sel = k; render(); ctxMenu(e, k); } }, h('span.fi' + (n.t === 'd' ? '.dir' : '')), k))));
      const n = sel && nd.ch[sel]; const full = (path === '/' ? '' : path) + '/' + sel;
      props.replaceChildren(...(n ? [h('b', { style: 'font-size:14px;word-break:break-all' }, sel), h('span.sub', n.t === 'd' ? 'Каталог' : 'Файл · ' + fmtB(n.size)), ...[['Права', n.perm + ' (' + toOct(n.perm) + ')'], ['Владелец', n.own + ':' + n.own], ['Изменён', n.mod], ['Путь', full]].flatMap(([k, v]) => [h('div.k', k), h('div.v', v)]), h('div.row', h('button.btn.sm', { onclick: () => openIt(sel) }, 'Открыть'), h('button.btn.sm', { onclick: () => permDlg(sel) }, 'Права…')), h('button.btn.sm', { onclick: () => download(sel) }, '↓ Скачать'), h('button.btn.sm.d', { onclick: () => del(sel) }, 'Удалить…')] : [h('span.sub', 'Выберите файл'), h('div.k', 'Каталог'), h('div.v', path), h('div.k', 'Элементов'), h('div.v', Object.keys(nd.ch).length)]));
      st.replaceChildren(h('span', its.length + ' элементов'), h('span.sp'), h('span', 'SFTP · ' + s.user + (sysPath(path) ? ' · системный каталог' : '')));
    }
    const toOct = p => [0, 3, 6].map(i => ['r', 'w', 'x'].reduce((a, c, j) => a + (p[i + j] === c || p[i + j] === 't' ? [4, 2, 1][j] : 0), 0)).join('');
    function openIt(k) { const n = nodeAt(path).ch[k]; if (n.t === 'd') cd((path === '/' ? '' : path) + '/' + k); else if (n.text != null || n.size < 5e5) desk.open('editor', { file: (path === '/' ? '' : path) + '/' + k, fresh: true }); else toast('Файл больше 5 МиБ: только скачать', 1); }
    function ctxMenu(e, k) { desk.ctx(e, [['Открыть', () => openIt(k)], nodeAt(path).ch[k].t === 'f' ? ['Редактировать', () => openIt(k)] : null, ['Переименовать…', () => ren(k)], ['Права…', () => permDlg(k)], ['Скачать', () => download(k)], '-', ['Удалить…', () => del(k), 1]]); }
    function mk(t) { const inp = h('input.inp.mono', { value: t === 'd' ? 'новая-папка' : 'новый-файл.txt' }); modal({ title: t === 'd' ? 'Новая папка' : 'Новый файл', body: h('label.f', 'Имя', inp), w: 400, buttons: [{ text: 'Отмена' }, { text: 'Создать', cls: 'p', fn: () => { const nd = nodeAt(path); if (!inp.value || nd.ch[inp.value]) return toast('Имя занято или пусто', 1); nd.ch[inp.value] = t === 'd' ? mkD({}, s.user, 'rwxr-xr-x', 'сейчас') : mkF(0, s.user, 'rw-r--r--', 'сейчас', ''); desk.log('Создан ' + path + '/' + inp.value); sel = inp.value; render(); } }] }); }
    function ren(k) { const inp = h('input.inp.mono', { value: k }); modal({ title: 'Переименовать', body: h('label.f', 'Новое имя', inp), w: 400, buttons: [{ text: 'Отмена' }, { text: 'Переименовать', cls: 'p', fn: () => { const nd = nodeAt(path); if (inp.value && !nd.ch[inp.value]) { nd.ch[inp.value] = nd.ch[k]; delete nd.ch[k]; sel = inp.value; desk.log('Переименован ' + k + ' → ' + inp.value); render(); } } }] }); }
    function del(k) { const n = nodeAt(path).ch[k], full = (path === '/' ? '' : path) + '/' + k, cnt = n.t === 'd' ? Object.keys(n.ch).length : 0, crit = sysPath(full) || (n.t === 'd' && cnt > 0 && s.group === 'Prod');
      confirmDanger({ title: 'Удалить ' + (n.t === 'd' ? 'каталог' : 'файл') + '?', server: s.name, prod: s.group === 'Prod', typeName: crit ? s.name : null, okText: 'Удалить', lines: [full, n.t === 'd' ? `Внутри: ${cnt} элементов (рекурсивно)` : 'Размер: ' + fmtB(n.size), 'Действие необратимо.'], onOk: () => { if (n.own === 'root' && s.user !== 'root') { sudoDlg(() => finish()); } else finish(); function finish() { delete nodeAt(path).ch[k]; desk.log('Удалено ' + full); toast('Удалено: ' + k); sel = null; render(); } } }); }
    function sudoDlg(then) { const inp = h('input.inp.mono', { type: 'password', placeholder: 'пароль sudo' }); modal({ title: 'Нужны права root', icon: h('span.dot.off'), body: h('div.col', h('div.sub', 'Файл принадлежит root. Пароль хранится только в памяти сессии.'), inp), w: 420, buttons: [{ text: 'Отмена' }, { text: 'Продолжить', cls: 'p', fn: () => then() }] }); }
    function permDlg(k) { const n = nodeAt(path).ch[k]; const bits = n.perm.split('').map(c => c !== '-'); const cbs = bits.map((b, i) => h('input', { type: 'checkbox', checked: b, onchange: upd })); const oct = h('span.mono.cy'); const rec = h('input', { type: 'checkbox' });
      function upd() { oct.textContent = [0, 3, 6].map(i => (cbs[i].checked ? 4 : 0) + (cbs[i + 1].checked ? 2 : 0) + (cbs[i + 2].checked ? 1 : 0)).join(''); } upd();
      const grid = h('div', { style: 'display:grid;grid-template-columns:90px repeat(3,60px);gap:6px 8px;align-items:center' }, h('span'), ...['чтение', 'запись', 'запуск'].map(t => h('span.cap', t)), ...['Владелец', 'Группа', 'Остальные'].flatMap((r, ri) => [h('span', r), ...cbs.slice(ri * 3, ri * 3 + 3)]));
      modal({ title: 'Права: ' + k, body: h('div.col', grid, h('div.row', 'Восьмеричное: ', oct), n.t === 'd' ? h('label.row', rec, 'Применить рекурсивно') : null), w: 440, buttons: [{ text: 'Отмена' }, { text: 'Применить', cls: 'p', fn: () => { const np = cbs.map((c, i) => c.checked ? 'rwx'[i % 3] : '-').join(''); const full = (path === '/' ? '' : path) + '/' + k; const ap = () => { n.perm = np; desk.log('chmod ' + oct.textContent + ' ' + full); render(); toast('Права изменены: ' + oct.textContent); }; if (rec.checked || sysPath(full)) confirmDanger({ title: 'Изменить права ' + (rec.checked ? 'рекурсивно' : 'системного файла') + '?', server: s.name, prod: s.group === 'Prod', lines: [full, 'chmod ' + (rec.checked ? '-R ' : '') + oct.textContent], okText: 'Применить', onOk: ap }); else ap(); } }] }); }
    /* передачи */
    function transfer(label, name, size, done) { const bar = h('div.bar.cy', h('i')), pct = h('span.cy', '0%'), spd = h('span.sub', ''); const row = h('div.xfer', h('span', { style: 'width:130px;overflow:hidden;text-overflow:ellipsis' }, label + ' ' + name), bar, pct, spd, h('span.sub', fmtB(size))); xf.append(row); let p = 0; const iv = setInterval(() => { p = Math.min(100, p + rnd(4, 11)); bar.firstChild.style.width = p + '%'; pct.textContent = Math.round(p) + '%'; spd.textContent = rnd(8, 18).toFixed(1) + ' МБ/с'; if (p >= 100) { clearInterval(iv); pct.textContent = '✓'; spd.textContent = ''; bar.className = 'bar'; done && done(); setTimeout(() => row.remove(), 2500); } }, 220); win.cleanup.push(() => clearInterval(iv)); }
    function upload(files) { files.forEach(([name, size]) => { const nd = nodeAt(path); transfer('↑', name, size, () => { nd.ch[name] = mkF(size, s.user, 'rw-r--r--', 'сейчас'); desk.log('Загружен ' + path + '/' + name); toast('Загружено: ' + name); render(); }); }); }
    function download(k) { const n = nodeAt(path).ch[k]; transfer('↓', k, n.size || 4096, () => toast('Скачано: ' + k + ' → ~/Загрузки')); }
    list.addEventListener('dragover', e => { e.preventDefault(); list.classList.add('drop'); }); list.addEventListener('dragleave', () => list.classList.remove('drop'));
    list.addEventListener('drop', e => { e.preventDefault(); list.classList.remove('drop'); const fl = [...(e.dataTransfer.files || [])].map(f => [f.name, f.size]); upload(fl.length ? fl : [['dropped.bin', 5e6]]); });
    render(); win.setTitle('Файлы', '· ' + s.name + ':' + path);
    return { cd: p => cd(p), tab: (t, o) => o.path && cd(o.path) };
  } });

/* ============ РЕДАКТОР ============ */
reg({ id: 'editor', name: 'Редактор', ico: 'Ed', c: '#8bd3ff', w: 720, h: 480, cat: 'Файлы', desc: 'Текстовый редактор файлов', hidden: true,
  build(win, desk, opts) {
    const s = desk.srv, nd = fsGet(desk.fs, opts.file), name = opts.file.split('/').pop(); let dirty = false;
    const ta = h('textarea.editor', { spellcheck: false, value: nd.text != null ? nd.text : '# бинарный/пустой файл\n', oninput: () => { dirty = true; mark(); } });
    const pos = h('span', 'Стр 1, Ст 1'), dm = h('span.am', '');
    const needRoot = nd.own === 'root' && s.user !== 'root';
    function mark() { win.setTitle('Редактор', '· ' + opts.file + (dirty ? ' ●' : '')); dm.textContent = dirty ? 'изменён' : ''; }
    function save() { const go = () => { nd.text = ta.value; nd.size = ta.value.length; nd.mod = 'сейчас'; dirty = false; mark(); desk.log('Сохранён ' + opts.file); toast('Сохранено: ' + name); }; if (needRoot) { const inp = h('input.inp.mono', { type: 'password', placeholder: 'пароль sudo' }); modal({ title: 'Сохранить как root', body: h('div.col', h('div.sub', 'Файл принадлежит root, нужен sudo.'), inp), w: 400, buttons: [{ text: 'Отмена' }, { text: 'Сохранить', cls: 'p', fn: go }] }); } else go(); }
    ta.addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); save(); } if (e.key === 'Tab') { e.preventDefault(); document.execCommand('insertText', false, '  '); } });
    ta.addEventListener('keyup', () => { const l = ta.value.slice(0, ta.selectionStart).split('\n'); pos.textContent = 'Стр ' + l.length + ', Ст ' + (l[l.length - 1].length + 1); });
    win.body.append(h('div.tbar', h('button.btn.sm.p', { onclick: save }, needRoot ? 'Сохранить как root' : 'Сохранить (Ctrl+S)'), h('button.btn.sm', { onclick: () => toast('Поиск/замена: Ctrl+F (прототип)') }, 'Найти'), h('span.sp'), needRoot ? h('span.chip.am', 'владелец root') : null), ta, h('div.wstat', pos, dm, h('span.sp'), h('span', 'UTF-8 · LF')));
    mark(); setTimeout(() => ta.focus(), 50);
    return { beforeClose: (go) => { if (!dirty) return false; modal({ title: 'Несохранённые изменения', body: h('div', 'Файл ' + name + ' изменён. Закрыть без сохранения?'), w: 420, buttons: [{ text: 'Отмена' }, { text: 'Закрыть без сохранения', cls: 'd', fn: go }, { text: 'Сохранить', cls: 'p', fn: () => { save(); go(); } }] }); return true; } };
  } });

/* ============ МОНИТОР ============ */
reg({ id: 'monitor', name: 'Монитор ресурсов', ico: 'Mn', c: '#00e5ff', w: 900, h: 600, cat: 'Мониторинг', single: true, desc: 'CPU, RAM, сеть, процессы', kw: 'cpu ram htop процессы kill',
  build(win, desk) {
    const s = desk.srv, m = desk.m; let procs = PROCS(), sel = null, sortK = 'cpu', sortD = -1, filt = ''; const cur = { v: 'ov' };
    const c1 = h('canvas', { 'data-h': 90 }), c2 = h('canvas', { 'data-h': 90 }), c3 = h('canvas', { 'data-h': 90 }), cores = h('div.cores'), dsk = h('div.col', { style: 'gap:6px' });
    const hdr = (t, v) => h('div.h', h('span.cap', t), v);
    const cpuV = h('b.mono'), ramV = h('b.mono'), netV = h('b.mono'), loadEl = h('span.mono.sub');
    const ov = h('div.pane', h('div.mgrid', h('div.mcard', hdr('CPU', cpuV), c1), h('div.mcard', hdr('RAM ' + s.ram + ' ГБ', ramV), c2)), h('div.mcard', hdr('Ядра', loadEl), cores), h('div.mgrid', h('div.mcard', hdr('Сеть eth0', netV), c3), h('div.mcard', hdr('Диск ввод/вывод', ''), dsk)));
    const tb = h('tbody'), pFilter = h('input.inp', { placeholder: 'Фильтр процессов', style: 'width:200px', oninput: e => { filt = e.target.value.toLowerCase(); renderP(); } });
    const pbar = h('div.tbar', pFilter, h('span.sp'), ...[['Завершить', 'TERM'], ['Kill -9', 'KILL'], ['Сигнал STOP', 'STOP'], ['Renice…', 'RENICE']].map(([t, k]) => h('button.btn.sm' + (k === 'KILL' ? '.d' : ''), { onclick: () => act(k) }, t)));
    const pt = h('table.t.mono', h('thead', h('tr', ...[['pid', 'PID', 60], ['name', 'Имя', 140], ['user', 'Польз.', 90], ['cpu', 'CPU%', 70], ['mem', 'RAM', 80], ['st', 'Ст', 40]].map(([k, t, w]) => h('th', { style: `width:${w}px;cursor:pointer`, onclick: () => { sortD = sortK === k ? -sortD : -1; sortK = k; renderP(); } }, t + (sortK === k ? (sortD < 0 ? ' ▾' : ' ▴') : ''))), h('th', 'Команда'))), tb);
    const procPane = h('div', { style: 'display:none;flex:1;min-height:0;flex-direction:column' }, pbar, h('div.scroll', { style: 'flex:1' }, pt));
    const netPane = h('div.pane', { style: 'display:none' }, h('table.t.mono', h('thead', h('tr', ...['Интерфейс', 'Статус', 'IPv4', '↓ всего', '↑ всего'].map(x => h('th', x)))), h('tbody', [['lo', 'up', '127.0.0.1', '1.2 ГБ', '1.2 ГБ'], ['eth0', 'up', s.host.match(/^[\d.]+$/) ? s.host : '203.0.113.7', '412 ГБ', '96 ГБ'], ['docker0', 'up', '172.17.0.1', '3.4 ГБ', '5.1 ГБ']].map(r => h('tr', r.map((x, i) => h('td' + (i === 1 ? '.gr' : ''), x)))))));
    const t = tabsBar([['ov', 'Обзор'], ['pr', 'Процессы'], ['nt', 'Сеть']], cur, k => { ov.style.display = k === 'ov' ? '' : 'none'; procPane.style.display = k === 'pr' ? 'flex' : 'none'; netPane.style.display = k === 'nt' ? '' : 'none'; tickDraw(); });
    win.body.append(h('div.tbar', t, h('span.sp'), h('span.sub.mono', 'обновление'), h('select.inp', { style: 'width:70px', onchange: e => iv = +e.target.value }, h('option', { value: 1 }, '1 с'), h('option', { value: 2 }, '2 с'), h('option', { value: 5 }, '5 с')), h('button.btn.sm', { onclick: e => { paused = !paused; e.currentTarget.textContent = paused ? '▶' : '⏸'; } }, '⏸')), ov, procPane, netPane, h('div.wstat', h('span', 'данные: /proc'), h('span', 'права: ' + s.user), h('span.sp'), h('span.mono', 'процессов: ' + procs.length)));
    let iv = 1, paused = false, n = 0;
    function renderP() {
      procs.sort((a, b) => (typeof a[sortK] === 'string' ? a[sortK].localeCompare(b[sortK]) : a[sortK] - b[sortK]) * sortD);
      tb.replaceChildren(...procs.filter(p => !filt || (p.name + p.user + p.cmd + p.pid).toLowerCase().includes(filt)).map(p => h('tr' + (sel === p.pid ? '.sel' : ''), { onclick: () => { sel = p.pid; renderP(); }, oncontextmenu: e => { e.preventDefault(); sel = p.pid; renderP(); desk.ctx(e, [['Завершить (TERM)', () => act('TERM')], ['Принудительно (KILL)', () => act('KILL'), 1], ['Renice…', () => act('RENICE')]]); } }, h('td.sub', p.pid), h('td', { style: 'font-weight:700' }, p.name), h('td.sub', p.user), h('td' + (p.cpu > 10 ? '.am' : ''), p.cpu.toFixed(1)), h('td', fmtB(p.mem)), h('td.sub', p.st), h('td.sub', { style: 'max-width:320px' }, p.cmd))));
    }
    function act(k) {
      const p = procs.find(x => x.pid === sel); if (!p) return toast('Выберите процесс', 1);
      if (k === 'RENICE') { const inp = h('input', { type: 'range', min: -20, max: 19, value: 0, oninput: () => v.textContent = inp.value }), v = h('b.mono.cy', '0'); return modal({ title: 'Renice: ' + p.name + ' (' + p.pid + ')', body: h('div.col', h('div.sub', 'nice −20 (высший приоритет) … 19 (низший). Значения < 0 требуют root.'), h('div.row', inp, v)), w: 440, buttons: [{ text: 'Отмена' }, { text: 'Применить', cls: 'p', fn: () => { if (+inp.value < 0 && s.user !== 'root') toast('Нужны права root для nice < 0', 1); else { desk.log('renice ' + inp.value + ' ' + p.pid); toast('nice ' + inp.value + ' для ' + p.name); } } }] }); }
      const crit = p.pid === 1 || p.name === 'sshd' || p.user === 'root'; const lines = [p.name + ' (PID ' + p.pid + ') · ' + p.user, p.cmd, 'Сигнал: SIG' + k + (k === 'KILL' ? ' — процесс не сможет сохранить данные' : '')]; if (p.name === 'sshd') lines.push('⚠ Может оборвать SSH-соединения, в том числе ваше.'); if (p.pid === 1) lines.push('⚠ PID 1 — init. Сервер может стать нестабильным.');
      confirmDanger({ title: 'Отправить SIG' + k + '?', server: s.name, prod: s.group === 'Prod', typeName: crit ? s.name : null, okText: 'Отправить SIG' + k, lines, onOk: () => { if (p.user === 'root' && s.user !== 'root') return toast('Операция не позволена (нужен sudo)', 1); desk.log('kill -' + k + ' ' + p.pid + ' (' + p.name + ')'); if (k !== 'STOP') procs = procs.filter(x => x.pid !== p.pid); else p.st = 'T'; sel = null; toast('SIG' + k + ' → ' + p.name); renderP(); } });
    }
    function tickDraw() {
      if (ov.style.display !== 'none') { drawChart(c1, [{ d: m.cpu, color: '#3dff8a' }], 100); drawChart(c2, [{ d: m.ram, color: '#ffc53d' }], 100); drawChart(c3, [{ d: m.rx, color: '#00e5ff' }, { d: m.tx, color: '#b388ff' }], 2600);
        cpuV.textContent = Math.round(m.cpu[59]) + '%'; cpuV.style.color = col(m.cpu[59]); ramV.textContent = (s.ram * m.ram[59] / 100).toFixed(1) + ' / ' + s.ram + ' ГБ'; netV.innerHTML = '<span class="cy">↓ ' + (m.rx[59] / 1000).toFixed(2) + ' МБ/с</span> <span class="pu">↑ ' + (m.tx[59] / 1000).toFixed(2) + ' МБ/с</span>'; loadEl.textContent = 'load ' + m.load.map(x => x.toFixed(2)).join(' ') + ' · процессов ' + m.procs;
        cores.replaceChildren(...m.cores.map((v, i) => h('div', '#' + i + ' ' + Math.round(v) + '%', h('div.bar' + (v > 80 ? '.rd' : v > 55 ? '.am' : ''), h('i', { style: 'width:' + v + '%' }))))); dsk.replaceChildren(...[['чтение', rnd(2, 9), 'cy'], ['запись', rnd(1, 22), 'am']].map(([t, v, c]) => h('div.row', h('span.sub.mono', { style: 'width:60px' }, t), h('div.bar.' + c + '.grow', h('i', { style: 'width:' + v * 3 + '%' })), h('span.mono', v.toFixed(1) + ' МБ/с')))); }
      if (cur.v === 'pr' && !(++n % 2)) { procs.forEach(p => { p.cpu = clamp(p.cpu + rnd(-1.5, 1.5), 0, 60) * (p.st === 'T' ? 0 : 1); }); renderP(); }
    }
    let tcount = 0; win.cleanup.push(desk.on(ev => { if (ev === 'tick' && !win.min && !paused && (++tcount % iv === 0)) tickDraw(); }));
    win.body.addEventListener('wresize', tickDraw); renderP(); setTimeout(tickDraw, 30);
    return { tab: k => t.set(k === 'proc' ? 'pr' : 'ov') };
  } });

/* ============ ДИСКИ ============ */
reg({ id: 'disks', name: 'Анализ дисков', ico: 'Dk', c: '#ffc53d', w: 920, h: 600, cat: 'Мониторинг', single: true, desc: 'Разделы, карта занятости, крупные файлы', kw: 'df ncdu место treemap sunburst',
  build(win, desk) {
    const s = desk.srv; let parts = [{ dev: '/dev/vda1', fs: 'ext4', mp: '/', size: 80, used: 52, inode: 12 }, { dev: '/dev/vdb1', fs: 'xfs', mp: '/data', size: 500, used: 465, inode: 4 }, { dev: 'tmpfs', fs: 'tmpfs', mp: '/dev/shm', size: 4, used: 0, inode: 1 }];
    let tree = DISK_TREE(), stack = [tree], mode = 'treemap', scanned = false; const cur = { v: 'parts' };
    const partsEl = h('div.pane'), mapEl = h('div.pane', { style: 'display:none' }), bigEl = h('div.pane', { style: 'display:none' }), cleanEl = h('div.pane', { style: 'display:none' });
    function renderParts() { const b = p => { const u = p.used / p.size * 100; return h('div.part', h('b', p.dev), h('span.sub', p.fs), h('span', p.mp), h('span', p.size + ' ГБ'), h('div.bar' + (u > 90 ? '.rd' : u > 65 ? '.am' : ''), h('i', { style: 'width:' + u + '%' })), h('b' + (u > 90 ? '.rd' : u > 65 ? '.am' : '.gr'), Math.round(u) + '%')); };
      partsEl.replaceChildren(h('div.box', h('div.part.cap', { style: 'font-family:var(--ui)' }, h('span', 'Устройство'), h('span', 'ФС'), h('span', 'Точка'), h('span', 'Размер'), h('span', 'Занято'), h('span', '')), ...parts.map(b)), h('div.cap', 'Блочные устройства'), h('div.fp', 'vda      80G  disk\n└─vda1   80G  part  /\nvdb     500G  disk\n└─vdb1  500G  part  /data'), h('div.row', h('span.chip.gr', 'SMART: PASSED'), h('span.chip.cy', 'inode / ' + parts[0].inode + '%'), h('span.sp'), h('button.btn.sm', { onclick: () => { cur.v = 'map'; t.set('map'); } }, 'Анализировать /data →')));
    }
    /* treemap / sunburst */
    const cv = h('canvas', { 'data-h': 330, style: 'cursor:pointer;border-radius:8px;background:rgba(var(--pan),.35)' }); let rects = [];
    const info = h('div.sub.mono', { style: 'min-height:18px' }), crumbs = h('div.row.mono', { style: 'gap:6px;flex-wrap:wrap' }), side = h('div.col', { style: 'width:250px;flex:none;gap:4px' });
    const scanBar = h('div.bar.cy', h('i')), scanTxt = h('span.mono.sub');
    function squarify(items, x, y, w, hh, out) { const tot = items.reduce((a, b) => a + b.s, 0); if (!tot || !items.length) return; let hor = w >= hh, off = 0; items = items.slice().sort((a, b) => b.s - a.s);
      (function lay(its, x, y, w, hh) { if (!its.length) return; if (its.length === 1) { out.push({ n: its[0], x, y, w, h: hh }); return; } const T = its.reduce((a, b) => a + b.s, 0); let acc = 0, k = 0; const half = T / 2; while (k < its.length - 1 && acc + its[k].s <= half) acc += its[k++].s; if (k === 0) { acc = its[0].s; k = 1; } const a = its.slice(0, k), b = its.slice(k), f = acc / T; if (w >= hh) { lay(a, x, y, w * f, hh); lay(b, x + w * f, y, w * (1 - f), hh); } else { lay(a, x, y, w, hh * f); lay(b, x, y + hh * f, w, hh * (1 - f)); } })(items, x, y, w, hh); }
    function drawMap() {
      const cd = stack[stack.length - 1], { g, w, h: hh } = fitCanvas(cv); g.clearRect(0, 0, w, hh); rects = [];
      if (mode === 'treemap') { const out = []; squarify(cd.ch || [cd], 0, 0, w, hh, out); out.forEach(r => { const c = KIND_COLOR[kindOf(r.n)]; g.fillStyle = c + '33'; g.strokeStyle = c; g.lineWidth = 1; g.beginPath(); g.roundRect(r.x + 1.5, r.y + 1.5, r.w - 3, r.h - 3, 4); g.fill(); g.stroke(); if (r.w > 60 && r.h > 28) { g.fillStyle = '#e6eaf2'; g.font = '600 12px Inter,sans-serif'; g.fillText(r.n.n, r.x + 8, r.y + 18); g.fillStyle = '#8b93a7'; g.font = '11px monospace'; g.fillText(fmtB(r.n.s), r.x + 8, r.y + 33); } rects.push({ ...r, t: 'r' }); }); }
      else { const cx = w / 2, cy = hh / 2, R = Math.min(w, hh) / 2 - 6; rects = [];
        const ring = (nd, a0, a1, depth) => { if (!nd.ch || depth > 3) return; let a = a0; nd.ch.forEach(c => { const sw = (a1 - a0) * c.s / nd.s, r0 = R * (depth) / 4 + (depth ? 2 : 0), r1 = R * (depth + 1) / 4; const col = KIND_COLOR[kindOf(c)]; g.beginPath(); g.arc(cx, cy, r1 - 1, a, a + sw); g.arc(cx, cy, r0, a + sw, a, true); g.closePath(); g.fillStyle = col + (depth ? '44' : '66'); g.fill(); g.strokeStyle = col; g.lineWidth = 1; g.stroke(); rects.push({ t: 'a', n: c, a0: a, a1: a + sw, r0, r1: r1 - 1 }); if (sw * r1 > 50) { const am = a + sw / 2, rm = (r0 + r1) / 2; g.fillStyle = '#e6eaf2'; g.font = '600 11px Inter,sans-serif'; g.textAlign = 'center'; g.fillText(c.n, cx + Math.cos(am) * rm, cy + Math.sin(am) * rm + 4); g.textAlign = 'start'; } ring(c, a, a + sw, depth + 1); a += sw; }); };
        ring(cd, -Math.PI / 2, Math.PI * 1.5, 0); g.fillStyle = '#e6eaf2'; g.font = '700 15px Inter,sans-serif'; g.textAlign = 'center'; g.fillText(fmtB(cd.s), cx, cy + 2); g.font = '11px monospace'; g.fillStyle = '#8b93a7'; g.fillText(cd.n, cx, cy + 18); g.textAlign = 'start'; }
      crumbs.replaceChildren(...stack.flatMap((n, i) => [i ? h('span.sub', '›') : null, h('span' + (i === stack.length - 1 ? '.cy' : ''), { style: 'cursor:pointer', onclick: () => { stack = stack.slice(0, i + 1); drawMap(); } }, n.n === '/' ? '/' : n.n)].filter(Boolean)));
      side.replaceChildren(h('div.cap', 'Крупнейшие в ' + cd.n), ...(cd.ch || []).slice().sort((a, b) => b.s - a.s).slice(0, 9).map(c => h('div.row', { style: 'cursor:pointer;font:12px var(--mono)', onclick: () => { if (c.ch) { stack.push(c); drawMap(); } } }, h('i', { style: `width:9px;height:9px;border-radius:2px;background:${KIND_COLOR[kindOf(c)]}` }), h('span.grow', { style: 'overflow:hidden;text-overflow:ellipsis' }, c.n), h('span.sub', fmtB(c.s)), h('span.sub', Math.round(c.s / cd.s * 100) + '%'))), h('div.cap', { style: 'margin-top:6px' }, 'Типы'), h('div.legend', Object.entries(KIND_NAME).map(([k, n]) => h('span', h('i', { style: 'background:' + KIND_COLOR[k] }), n))));
    }
    const hit = e => { const r = cv.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top; if (mode === 'treemap') return rects.find(q => x >= q.x && x <= q.x + q.w && y >= q.y && y <= q.y + q.h); const cx = r.width / 2, cy = r.height / 2, dx = x - cx, dy = y - cy, rad = Math.hypot(dx, dy); let an = Math.atan2(dy, dx); if (an < -Math.PI / 2) an += Math.PI * 2; return rects.find(q => rad >= q.r0 && rad <= q.r1 && an >= q.a0 && an <= q.a1); };
    cv.addEventListener('mousemove', e => { const q = hit(e); info.textContent = q ? q.n.n + ' · ' + fmtB(q.n.s) + (q.n.ch ? ' · клик — войти' : '') : ''; });
    cv.addEventListener('click', e => { const q = hit(e); if (q && q.n.ch) { stack.push(q.n); drawMap(); } });
    function scan() { scanned = false; renderMapTop(); let p = 0; const iv = setInterval(() => { p += rnd(6, 14); scanBar.firstChild.style.width = Math.min(p, 100) + '%'; scanTxt.textContent = 'Скан ' + Math.min(100, Math.round(p)) + '% · ' + ['/var/lib', '/var/log', '/home', '/usr'][Math.min(3, Math.floor(p / 26))] + ' · ionice 3'; if (p >= 100) { clearInterval(iv); scanned = true; tree = DISK_TREE(); stack = [tree]; renderMapTop(); } }, 220); win.cleanup.push(() => clearInterval(iv)); }
    function renderMapTop() {
      mapEl.replaceChildren(h('div.row', crumbs, h('span.sp'), h('div.seg', ...[['treemap', 'Treemap'], ['sun', 'Sunburst']].map(([k, t]) => h('button' + (k === mode ? '.on' : ''), { onclick: () => { mode = k; renderMapTop(); } }, t))), h('button.btn.sm', { onclick: scan }, 'Пересканировать')),
        scanned ? h('div.row', { style: 'align-items:flex-start' }, h('div.grow', cv, info), side) : h('div.col', h('div.row', scanBar, scanTxt), h('div.sub', 'Сканирование корня нагружает диск — выполняется с низким приоритетом (ionice).')));
      if (scanned) setTimeout(drawMap, 0); }
    /* крупные файлы */
    let thr = 1; function leaves(n, p, out) { (n.ch || []).forEach(c => { const fp = p + '/' + c.n; if (c.ch) leaves(c, fp, out); else out.push({ p: fp, s: c.s, k: c.kind }); }); return out; }
    function renderBig() { const L = leaves(DISK_TREE(), '', []).filter(x => x.s >= thr * 1e9).sort((a, b) => b.s - a.s); const lab = h('b.mono.cy', '≥ ' + thr + ' ГБ');
      bigEl.replaceChildren(h('div.row', h('span.sub', 'Порог'), h('input', { type: 'range', min: 0.3, max: 6, step: .1, value: thr, oninput: e => { thr = +e.target.value; renderBig(); } }), lab, h('span.sp'), h('span.sub', L.length + ' файлов')), h('table.t.mono', h('thead', h('tr', h('th', 'Путь'), h('th', 'Тип'), h('th', 'Размер'), h('th', ''))), h('tbody', L.map(x => h('tr', h('td', x.p), h('td', h('span.chip', { style: '--c:' + KIND_COLOR[x.k] }, KIND_NAME[x.k])), h('td', fmtB(x.s)), h('td', h('button.btn.sm', { onclick: () => { win.close(); desk.open('files', { path: x.p.slice(0, x.p.lastIndexOf('/')) || '/' }); } }, 'В файлах'), ' ', h('button.btn.sm.d', { onclick: () => confirmDanger({ title: 'Удалить ' + x.p.split('/').pop() + '?', server: s.name, prod: s.group === 'Prod', typeName: s.group === 'Prod' ? s.name : null, lines: [x.p, 'Размер: ' + fmtB(x.s)], okText: 'Удалить', onOk: () => { desk.log('rm ' + x.p); toast('Удалено ' + fmtB(x.s)); parts[1].used = Math.max(0, parts[1].used - x.s / 1e9); renderParts(); } }) }, 'Удалить…')))))));
    }
    /* очистка */
    const CL = [['Кэш apt (/var/cache/apt)', 1.9e9, 'apt clean'], ['Журналы journald старше 7 дней', 3.1e9, 'journalctl --vacuum-time=7d'], ['Старые ядра (3 пакета)', 0.9e9, 'apt autoremove --purge'], ['Docker: неиспользуемые образы', 8.4e9, 'docker image prune -a'], ['Ротированные логи nginx', 1.2e9, 'logrotate -f']];
    function renderClean() { const cbs = CL.map(() => h('input', { type: 'checkbox', checked: true, onchange: upd })), tot = h('b.mono.cy');
      function upd() { tot.textContent = fmtB(CL.reduce((a, c, i) => a + (cbs[i].checked ? c[1] : 0), 0)); } upd();
      cleanEl.replaceChildren(h('div.sub', 'Безопасные подсказки очистки с оценкой освобождаемого места.'), h('div.box', ...CL.map((c, i) => h('div.row', { style: 'padding:9px 12px;border-top:1px solid rgba(255,255,255,.05)' }, cbs[i], h('div.grow', h('div', c[0]), h('div.mono.sub', { style: 'font-size:11px' }, c[2])), h('b.mono', fmtB(c[1]))))), h('div.row', h('span', 'Будет освобождено: '), tot, h('span.sp'), h('button.btn.d', { onclick: () => { const sel = CL.filter((c, i) => cbs[i].checked); if (!sel.length) return; const tt = sel.reduce((a, c) => a + c[1], 0); confirmDanger({ title: 'Очистить ' + fmtB(tt) + '?', server: s.name, prod: s.group === 'Prod', typeName: s.group === 'Prod' ? s.name : null, okText: 'Очистить', lines: sel.map(c => '$ ' + c[2]), onOk: () => { desk.log('Очистка диска: ' + fmtB(tt)); parts[0].used = Math.max(1, parts[0].used - tt / 1e9); renderParts(); desk.notify('cy', 'Очистка завершена', 'Освобождено ' + fmtB(tt)); } }); } }, 'Очистить…')));
    }
    const t = tabsBar([['parts', 'Разделы'], ['map', 'Карта'], ['big', 'Крупные файлы'], ['clean', 'Очистка']], cur, k => { partsEl.style.display = k === 'parts' ? '' : 'none'; mapEl.style.display = k === 'map' ? '' : 'none'; bigEl.style.display = k === 'big' ? '' : 'none'; cleanEl.style.display = k === 'clean' ? '' : 'none'; if (k === 'map' && !scanned && !scanBar.dataset.s) { scanBar.dataset.s = 1; scan(); } if (k === 'map') renderMapTop(); if (k === 'big') renderBig(); if (k === 'clean') renderClean(); });
    win.body.append(h('div.tbar', t, h('span.sp'), h('span.sub.mono', 'путь'), h('input.inp.mono', { value: '/', style: 'width:120px' })), partsEl, mapEl, bigEl, cleanEl, h('div.wstat', h('span', 'df · lsblk · du'), h('span', 'права: ' + s.user)));
    win.body.addEventListener('wresize', () => scanned && cur.v === 'map' && drawMap());
    renderParts(); return { tab: k => t.set(k) };
  } });
