'use strict';
/* Sherpa prototype — ядро UI: DOM-хелперы, модальные окна, оконный менеджер, рабочий стол */
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
function h(tag, attrs, ...kids) {
  const m = tag.match(/^([a-z0-9]*)((?:[.#][\w-]+)*)/i);
  const el = document.createElement(m[1] || 'div');
  (m[2].match(/[.#][\w-]+/g) || []).forEach(p => p[0] === '.' ? el.classList.add(p.slice(1)) : (el.id = p.slice(1)));
  if (attrs && (typeof attrs !== 'object' || attrs instanceof Node || Array.isArray(attrs))) { kids.unshift(attrs); attrs = null; }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'style') typeof v === 'string' ? el.style.cssText = v : Object.assign(el.style, v);
    else if (k === 'html') el.innerHTML = v;
    else if (k === 'class') v.split(' ').forEach(c => c && el.classList.add(c));
    else if (k === 'value') el.value = v;
    else if (k in el && typeof v !== 'string' || k === 'checked' || k === 'disabled') el[k] = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  const add = k => { if (k == null || k === false) return; if (Array.isArray(k)) k.forEach(add); else el.append(k.nodeType ? k : document.createTextNode(k)); };
  kids.forEach(add);
  return el;
}
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const rnd = (a, b) => a + Math.random() * (b - a);
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const fmtB = n => n >= 1e9 ? (n / 1e9).toFixed(1) + ' ГБ' : n >= 1e6 ? (n / 1e6).toFixed(1) + ' МБ' : n >= 1e3 ? (n / 1e3).toFixed(0) + ' КБ' : n + ' Б';
const pad = n => String(n).padStart(2, '0');
const hhmm = (d = new Date()) => pad(d.getHours()) + ':' + pad(d.getMinutes());
const hhmmss = (d = new Date()) => hhmm(d) + ':' + pad(d.getSeconds());
function dot(cls) { return h('span.dot.' + cls); }
function ico(app, s = 30) { return h('span.ico', { style: `--s:${s}px;--c:${app.c}` }, app.ico); }

function toast(msg, err) {
  const t = h('div.toast' + (err ? '.err' : ''), msg);
  $('#toasts').append(t);
  setTimeout(() => { t.style.opacity = 0; t.style.transition = 'opacity .3s'; setTimeout(() => t.remove(), 300); }, 2800);
}

/* ---------- модальные окна ---------- */
function modal({ title, icon, body, buttons = [], cls = '', w, onclose, noClose }) {
  const root = $('#modal-root');
  const close = () => { scrim.remove(); document.removeEventListener('keydown', key, true); onclose && onclose(); };
  const key = e => { if (e.key === 'Escape' && !noClose && root.lastChild === scrim) { e.stopPropagation(); close(); } };
  const btns = h('div.btns', buttons.map(b => {
    const el = h('button.btn' + (b.cls ? '.' + b.cls : ''), { onclick: () => { if (!b.keep) close(); b.fn && b.fn(close, el); } }, b.text);
    if (b.disabled) el.disabled = true; b.ref && b.ref(el); return el;
  }));
  const m = h('div.modal' + (cls ? '.' + cls : ''), { style: w ? `--w:${w}px` : '' }, title && h('h3', icon, title), body, buttons.length ? btns : null);
  const scrim = h('div.scrim', { onmousedown: e => { if (e.target === scrim && !noClose) close(); } }, m);
  root.append(scrim); document.addEventListener('keydown', key, true);
  const f = $('.btn[autofocus],input,select', m) ; if (f) setTimeout(() => f.focus(), 30);
  return { close, el: m, btns };
}
/* опасные действия: диалог подтверждения как в DESKTOP.md §2.6 */
function confirmDanger({ title, lines = [], okText = 'Выполнить', typeName, server, onOk, prod }) {
  let okBtn, inp;
  const body = h('div.col',
    h('div', 'Сервер: ', h('b.mono', { style: 'font-size:15px' }, server || '—'), prod ? h('span.chip.rd', { style: 'margin-left:8px' }, 'PROD') : null),
    h('div.fp', lines.map(l => h('div', l))),
    typeName ? h('label.f', `Для подтверждения введите имя сервера «${typeName}»`, inp = h('input.inp.mono', { placeholder: typeName, oninput: () => okBtn.disabled = inp.value.trim() !== typeName })) : null);
  return modal({ title, icon: h('span.dot.err'), body, cls: 'danger', w: 520, buttons: [
    { text: 'Отмена', cls: '', fn: () => {} },
    { text: okText, cls: 'd', disabled: !!typeName, ref: b => okBtn = b, fn: () => onOk && onOk() }] });
}

/* ---------- данные ---------- */
const SERVERS = [
  { id: 'web-01', name: 'web-01', host: '10.0.0.11', port: 22, user: 'deploy', group: 'Prod', tags: ['nginx', 'eu'], status: 'on', lat: 38, checked: '2 мин назад', auth: 'key · ~/.ssh/id_ed25519', desc: 'Фронтенд-нода, Ubuntu 24.04', hue: '#ff4d6d', hk: 'SHA256:Zk3f9aQ1xTq…9QpA', os: 'Ubuntu 24.04 LTS', cores: 8, ram: 8 },
  { id: 'web-02', name: 'web-02', host: '10.0.0.12', port: 22, user: 'deploy', group: 'Prod', tags: ['nginx'], status: 'on', lat: 41, checked: '2 мин назад', auth: 'key · ~/.ssh/id_ed25519', desc: 'Фронтенд-нода #2', hue: '#ff4d6d', hk: 'SHA256:Hq91xB0pLw…2KmE', os: 'Ubuntu 24.04 LTS', cores: 8, ram: 8 },
  { id: 'db-main', name: 'db-main', host: '10.0.0.20', port: 22, user: 'postgres', group: 'Prod', tags: ['pg'], status: 'err', lat: 0, checked: '5 мин назад', auth: 'key · ~/.ssh/db_key', desc: 'PostgreSQL 16, основная БД', hue: '#ff4d6d', hk: 'SHA256:Pq02mN7sWe…8DdC', os: 'Debian 12', cores: 16, ram: 32, errText: 'Ошибка авторизации: ключ отклонён сервером' },
  { id: 'gw-eu', name: 'gw-eu', host: 'gw.example.com', port: 2200, user: 'admin', group: 'Prod', tags: ['eu'], status: 'on', lat: 87, checked: '10 мин назад', auth: 'key · ~/.ssh/id_rsa', desc: 'Шлюз Европа. Ключ хоста сменился!', hue: '#ff4d6d', hk: 'SHA256:Lm8uT7wPc…4XcB', os: 'Debian 12', cores: 4, ram: 4, hkChanged: true },
  { id: 'stage-api', name: 'stage-api', host: 'stage.example.com', port: 2222, user: 'ubuntu', group: 'Stage', tags: ['stage'], status: 'unk', lat: 0, checked: 'не проверялся', auth: 'пароль', desc: 'Stage API', hue: '#ffc53d', hk: '—', os: 'Ubuntu 22.04 LTS', cores: 4, ram: 8 },
  { id: 'stage-db', name: 'stage-db', host: '10.1.0.5', port: 22, user: 'postgres', group: 'Stage', tags: ['pg', 'stage'], status: 'on', lat: 52, checked: '1 ч назад', auth: 'key · ~/.ssh/id_ed25519', desc: 'Stage PostgreSQL', hue: '#ffc53d', hk: 'SHA256:Ab12Cd34Ef…56Gh', os: 'Debian 12', cores: 4, ram: 8 },
  { id: 'backup', name: 'backup', host: '192.168.1.5', port: 22, user: 'root', group: 'Infra', tags: ['infra'], status: 'off', lat: 0, checked: '1 ч назад', auth: 'пароль', desc: 'Хранилище бэкапов', hue: '#b388ff', hk: 'SHA256:Zz90Yy81Xx…72Ww', os: 'Debian 11', cores: 2, ram: 4, errText: 'Хост недоступен: таймаут 10 с' },
  { id: 'mon', name: 'mon', host: '192.168.1.9', port: 22, user: 'ops', group: 'Infra', tags: ['infra', 'nginx'], status: 'on', lat: 22, checked: '3 мин назад', auth: 'key · ~/.ssh/id_ed25519', desc: 'Мониторинг (Grafana)', hue: '#b388ff', hk: 'SHA256:Mm11Nn22Oo…33Pp', os: 'Ubuntu 24.04 LTS', cores: 4, ram: 8 },
];
const STATUS_TXT = { on: 'online', off: 'offline', err: 'auth failed', unk: 'не проверялся' };

/* ---------- рабочий стол + оконный менеджер ---------- */
const Desks = new Map();
const APPS = {}; // заполняется в apps.js
let ZTOP = 100;

class Desk {
  constructor(srv, opts = {}) {
    this.srv = srv; this.wins = []; this.listeners = new Set(); this.audit = []; this.notes = []; this.state = 'ready'; this.recent = [];
    this.m = { cpu: Array(60).fill(20), ram: Array(60).fill(60), rx: Array(60).fill(300), tx: Array(60).fill(40), cores: Array(srv.cores).fill(20), load: [.3, .2, .1], up: 41 * 86400 + 3 * 3600, procs: 214 };
    this.cascade = 0;
    this.el = h('div.screen');
    this.build();
    (this.seedNotes)();
    this.timer = setInterval(() => this.tick(), 1000);
    for (let i = 0; i < 70; i++) this.tick(true);
    this.tick();
    if (typeof initWidgets === 'function') initWidgets(this);
    this.connect(opts.fast);
  }
  seedNotes() {
    const n = (sev, t, d, app) => this.notes.push({ sev, t, d, app, time: hhmm(new Date(Date.now() - rnd(1, 120) * 60000)), unread: true });
    n('am', 'Диск / заполнен на 68%', 'Порог предупреждения 65%', 'disks');
    n('rd', 'Служба redis упала', 'Код выхода 1, 3 перезапуска за час', 'system');
    n('cy', 'Вход по SSH с нового IP', '5.6.7.8 · пользователь deploy', 'logs');
    this.notes.sort((a, b) => b.time.localeCompare(a.time));
  }
  build() {
    const s = this.srv, d = this;
    this.el.append(this.root = h('div.desk' + (s.group === 'Prod' ? '.prodstripe' : ''), { style: `--hue:${s.hue}`, onmousedown: e => { if (e.target === d.root || e.target.classList.contains('blobs') || e.target.classList.contains('icons') || e.target.classList.contains('wins')) { d.closeLauncher(); d.closeNotify(); d.blurAll(); $$('.dicon.sel', d.root).forEach(x => x.classList.remove('sel')); } }, oncontextmenu: e => { if (e.target === d.root || e.target.closest('.wins') === d.winsEl && e.target === d.winsEl || e.target.classList.contains('blobs') || e.target.classList.contains('icons')) { e.preventDefault(); d.ctx(e, [['Новый терминал', () => d.open('terminal')], ['Файлы здесь', () => d.open('files')], ['Монитор ресурсов', () => d.open('monitor')], '-', ['Виджеты…', () => widgetsDialog(d)], ['Обои…', () => d.open('settings')], ['Упорядочить значки', () => toast('Значки упорядочены')], '-', ['Свернуть все окна (Ctrl+D)', () => d.showDesktop()]]); } } },
      h('div.blobs'), h('div.wm', s.name),
      this.iconsEl = h('div.icons'),
      this.widgetsEl = h('div.widgets'),
      this.winsEl = h('div.wins'),
      this.snapEl = h('div.snap'),
      this.bannerEl = h('div.deskbanner.banner'),
      this.launcherEl = h('div.launcher'),
      this.notifyEl = h('div.nside'),
      this.ovlEl = h('div.ovl', { onclick: e => { if (e.target === this.ovlEl) this.ovlEl.classList.remove('open'); } }),
      this.connEl = h('div', { style: 'position:absolute;inset:0;display:grid;place-items:center;z-index:9700;background:rgba(2,4,10,.35);backdrop-filter:blur(4px)' }),
      this.taskEl = h('div.taskbar')));
    ['terminal', 'files', 'monitor', 'system'].forEach(id => {
      const a = APPS[id];
      this.iconsEl.append(h('div.dicon', { onclick: e => { $$('.dicon.sel', this.iconsEl).forEach(x => x.classList.remove('sel')); e.currentTarget.classList.add('sel'); }, ondblclick: () => this.open(id) }, ico(a, 44), a.name));
    });
    this.buildTaskbar(); this.buildLauncher(); this.buildNotify();
  }
  /* --- подключение и состояния --- */
  connect(fast) {
    const s = this.srv; this.connEl.style.display = 'grid';
    const steps = ['TCP :' + s.port, 'Host key', 'Вход ' + s.user + '@' + s.host, 'Готово'];
    const card = h('div.modal', { style: '--w:380px;align-items:center' }, h('div.mono.sub', 'подключение'), h('h3', 'Подключение к ' + s.name + '…'), h('div.fp', { style: 'width:100%' }, steps.map((t, i) => h('div', { id: 'st' + i }, '○ ' + t))));
    this.connEl.replaceChildren(card);
    if (s.status === 'err' || s.status === 'off') {
      setTimeout(() => { card.replaceChildren(h('h3.rd', 'Не удалось подключиться'), h('div', s.errText || 'Ошибка'), h('div.btns', h('button.btn', { onclick: () => closeDeskTab(s.id) }, 'Закрыть'), h('button.btn', { onclick: () => this.connect() }, 'Повторить'), h('button.btn.p', { onclick: () => { closeDeskTab(s.id); showHome(); toast('Откройте «Изменить» в карточке'); } }, 'Изменить подключение'))); }, fast ? 100 : 1000); return;
    }
    const ok = () => { this.connEl.style.display = 'none'; this.state = 'ready'; if (this.onReady) this.onReady(); };
    const run = (i) => { if (i >= steps.length) return ok(); const el = $('#st' + i, card); if (el) el.innerHTML = '<span class="gr">●</span> ' + esc(steps[i]);
      if (i === 1 && s.hkChanged && !s.hkAccepted) { setTimeout(() => hostKeyDialog(s, () => run(i + 1), () => { closeDeskTab(s.id); }), fast ? 50 : 300); return; }
      setTimeout(() => run(i + 1), fast ? 40 : 380); };
    run(0);
  }
  setState(st) {
    this.state = st; const b = this.bannerEl;
    if (st === 'lost') {
      this.root.classList.add('dim'); b.className = 'deskbanner banner err'; b.style.display = 'flex';
      let n = 5; const paint = () => { b.innerHTML = ''; b.append(h('span', `Соединение потеряно · переподключение через ${n} с`), h('button.btn.sm', { onclick: () => { n = 0; } }, 'Повторить сейчас')); };
      paint(); clearInterval(this.rc); this.rc = setInterval(() => { n--; if (n <= 0) { clearInterval(this.rc); this.setState('ready'); toast('Соединение восстановлено · ' + this.srv.name); } else paint(); }, 1000);
      this.emit('state');
    } else { this.root.classList.remove('dim'); b.style.display = 'none'; this.emit('state'); }
  }
  /* --- метрики --- */
  tick(silent) {
    const m = this.m, walk = (a, lo, hi, st) => { const v = clamp(a[a.length - 1] + rnd(-st, st), lo, hi); a.push(v); a.shift(); return v; };
    if (this.state === 'lost') return;
    walk(m.cpu, 4, 96, 9); walk(m.ram, 40, 80, 1.2); walk(m.rx, 20, 2400, 260); walk(m.tx, 5, 400, 60);
    m.cores = m.cores.map(c => clamp(c + rnd(-14, 14), 2, 99)); m.up += 1; m.load = [clamp(m.load[0] + rnd(-.05, .06), .02, 4), m.load[1], m.load[2]];
    const c = m.cpu[59], r = m.ram[59], rx = m.rx[59], tx = m.tx[59];
    if (!silent) {
      const t = this.tray; if (t) {
        t.cpu.textContent = Math.round(c) + '%'; t.cpuB.firstChild.style.width = c + '%'; t.cpuB.className = 'bar ' + (c > 80 ? 'rd' : c > 55 ? 'am' : '');
        t.ram.textContent = Math.round(r) + '%'; t.ramB.firstChild.style.width = r + '%'; t.ramB.className = 'bar ' + (r > 85 ? 'rd' : 'am');
        t.net.textContent = '↓' + (rx / 1000).toFixed(1) + 'M ↑' + (tx / 1000).toFixed(1) + 'M';
        t.up.textContent = Math.floor(m.up / 86400) + 'd'; t.lat.textContent = (this.srv.lat + Math.round(rnd(-3, 3))) + 'ms';
        const d = new Date(); t.clock.firstChild.textContent = hhmm(d);
      }
      this.emit('tick');
    }
  }
  on(fn) { this.listeners.add(fn); return () => this.listeners.delete(fn); }
  emit(ev) { this.listeners.forEach(f => { try { f(ev); } catch (e) { console.error(e); window.__errs && window.__errs.push(String(e && e.stack || e)); } }); }
  /* --- панель задач --- */
  buildTaskbar() {
    const d = this, t = this.taskEl; this.tray = {};
    const tg = (label, valKey, extra, cls, fn) => { const v = h('b'); this.tray[valKey] = v; const g = h('div.tg', { onclick: fn }, label, v, extra); return g; };
    this.lBtn = h('button.lbtn', { title: 'Меню (Ctrl+Space)', onclick: e => { e.stopPropagation(); d.toggleLauncher(); } }, 'S');
    this.appsEl = h('div.row', { style: 'gap:6px' });
    const cpuB = h('div.bar', h('i')), ramB = h('div.bar.am', h('i'));
    this.tray.cpuB = cpuB; this.tray.ramB = ramB;
    const clk = h('div.clock', { onclick: () => d.open('system', { tab: 'time' }), title: 'Время сервера' }, h('span', '14:02'), h('small', 'UTC+3'));
    this.tray.clock = clk;
    this.bell = h('div.tg', { style: 'cursor:pointer', onclick: e => { e.stopPropagation(); d.toggleNotify(); } }, 'Увед.', this.badge = h('span.badge', '0'));
    t.append(this.lBtn,
      h('button.srvbtn', { onclick: () => d.toggleOverview(), title: 'Обзор рабочих столов (Ctrl+Alt+Space)' }, dot(this.srv.status), this.srv.name, h('span.sub', '▾')),
      h('div.tsep'), this.appsEl,
      h('div.tray', tg('CPU ', 'cpu', cpuB, '', () => d.open('monitor')), tg('RAM ', 'ram', ramB, '', () => d.open('monitor')), tg('NET ', 'net', null, '', () => d.open('net')), tg('UP ', 'up', null, '', () => d.open('system')), tg('', 'lat', null, '', () => d.setState('lost')),
        this.bell, h('div.tsep'), clk));
    this.renderTaskbar();
  }
  renderTaskbar() {
    const pinned = ['terminal', 'files', 'monitor', 'disks', 'net', 'system'];
    const ids = [...pinned, ...this.wins.map(w => w.app.id).filter(i => !pinned.includes(i))].filter((v, i, a) => a.indexOf(v) === i);
    const act = this.wins.find(w => w.active && !w.min);
    this.appsEl.replaceChildren(...ids.map(id => {
      const a = APPS[id], ws = this.wins.filter(w => w.app.id === id);
      const el = h('div.tapp' + (ws.length ? '.run' : '') + (act && act.app.id === id ? '.act' : ''), { style: `--c:${a.c}`, title: a.name,
        onclick: () => { if (!ws.length) return this.open(id); if (ws.length === 1) { const w = ws[0]; (w.active && !w.min) ? w.minimize() : w.focus(); } else { const i = ws.findIndex(w => w.active); ws[(i + 1) % ws.length].focus(); } },
        oncontextmenu: e => { e.preventDefault(); this.ctx(e, [['Новое окно', () => this.open(id, { fresh: true })], ws.length ? ['Закрыть все окна (' + ws.length + ')', () => ws.slice().forEach(w => w.close())] : null, '-', ['Открепить от панели', () => toast('Закреплением можно управлять в «Настройках»')]]); } },
        ico(a, 30), ws.length > 1 ? h('span.cnt', ws.length) : null, h('span.ind'));
      return el;
    }));
    const un = this.notes.filter(n => n.unread).length; this.badge.textContent = un; this.badge.style.display = un ? '' : 'none';
  }
  ctx(e, items) {
    $$('.ctx').forEach(x => x.remove());
    const m = h('div.ctx', items.filter(Boolean).map(i => i === '-' ? h('hr') : h('div' + (i[2] ? '.dg' : ''), { onclick: () => { m.remove(); i[1](); } }, i[0])));
    const r = this.root.getBoundingClientRect();
    this.root.append(m); m.style.left = Math.min(e.clientX - r.left, r.width - 210) + 'px'; m.style.top = Math.min(e.clientY - r.top, r.height - items.length * 30 - 60) + 'px';
    const off = () => { m.remove(); document.removeEventListener('mousedown', off, true); }; setTimeout(() => document.addEventListener('mousedown', off, true), 0);
  }
  /* --- лаунчер --- */
  buildLauncher() {
    const d = this, L = this.launcherEl; this.cat = 'Избранное';
    this.search = h('input.inp.mono', { placeholder: 'Поиск: утилиты, службы, пакеты, файлы…', oninput: () => d.renderLauncher(), onkeydown: e => {
      if (e.key === 'Enter') { const f = $('.litem', L); if (f) f.click(); } if (e.key === 'Escape') d.closeLauncher(); } });
    this.catsEl = h('div.cats'); this.gridEl = h('div.grid');
    L.append(h('div.row', this.search, h('span.sub.mono', 'Ctrl+Space')), h('div.lbody', this.catsEl, this.gridEl),
      h('div', h('div.cap', { style: 'margin-bottom:5px' }, 'Недавние'), this.recentEl = h('div.recent')),
      h('div.foot', h('button.btn.sm', { onclick: () => { d.closeLauncher(); d.setState('lost'); } }, 'Переподключить'), h('button.btn.sm', { onclick: () => { d.closeLauncher(); toast('Рабочий стол заблокирован (прототип)'); } }, 'Блокировать'), h('span.sp'),
        h('button.btn.sm.d', { onclick: () => { d.closeLauncher(); d.powerDialog('reboot'); } }, 'Перезагрузить…'), h('button.btn.sm.d', { onclick: () => { d.closeLauncher(); d.powerDialog('shutdown'); } }, 'Выключить…'), h('button.btn.sm', { onclick: () => closeDeskTab(d.srv.id) }, 'Закрыть стол')));
    L.addEventListener('mousedown', e => e.stopPropagation());
    this.renderLauncher();
  }
  powerDialog(kind) {
    const s = this.srv, k = kind === 'reboot' ? 'Перезагрузка' : 'Выключение';
    confirmDanger({ title: k + ' сервера', server: s.name, prod: s.group === 'Prod', typeName: s.name, okText: k, lines: ['Все SSH-сессии и запущенные задачи будут прерваны.', kind === 'reboot' ? 'Сервер вернётся в сеть через 1–3 минуты.' : 'Включить сервер можно будет только вне Sherpa.'],
      onOk: () => { this.log(k + ' сервера'); toast(k + ' запущена…'); this.setState('lost'); } });
  }
  cats() { return ['Избранное', 'Система', 'Сеть', 'Файлы', 'Мониторинг', 'Данные', 'Безопасность', 'Разработка', 'Все']; }
  renderLauncher() {
    const q = this.search.value.trim().toLowerCase(), g = this.gridEl;
    this.catsEl.replaceChildren(...this.cats().map(c => h('div' + (c === this.cat && !q ? '.on' : ''), { onclick: () => { this.cat = c; this.search.value = ''; this.renderLauncher(); } }, c)));
    this.recentEl.replaceChildren(...['/etc/nginx/nginx.conf', 'systemctl restart nginx', 'docker ps'].map(r => h('span', { style: 'cursor:pointer', onclick: () => { this.closeLauncher(); if (r.startsWith('/')) this.open('files', { path: '/etc/nginx' }); else this.open('terminal', { cmd: r }); } }, r)));
    const all = Object.values(APPS).filter(a => !a.hidden);
    const item = (a, extra) => h('div.litem', { onclick: () => { this.closeLauncher(); extra ? extra() : this.open(a.id); } }, ico(a, 34), h('div', { style: 'min-width:0' }, h('div.nm', a.name), h('div.d', a.desc)));
    if (q) {
      const apps = all.filter(a => (a.name + ' ' + a.desc + ' ' + (a.kw || '')).toLowerCase().includes(q));
      const out = apps.map(a => item(a));
      const svc = SERVICES.filter(s => s.name.includes(q)).slice(0, 3).map(s => item({ ...APPS.system, name: s.name + '.service', desc: 'Служба · ' + s.state }, () => this.open('system', { tab: 'svc', q: s.name })));
      const pk = PACKAGES.filter(p => p.n.includes(q)).slice(0, 2).map(p => item({ ...APPS.system, name: p.n, desc: 'Пакет ' + p.v }, () => this.open('system', { tab: 'pkg', q: p.n })));
      const fl = FILE_INDEX.filter(p => p.toLowerCase().includes(q)).slice(0, 3).map(p => item({ ...APPS.files, name: p.split('/').pop(), desc: p }, () => this.open('files', { path: p.slice(0, p.lastIndexOf('/')) || '/' })));
      g.replaceChildren(...out, ...svc, ...pk, ...fl); if (!g.children.length) g.append(h('div.empty', 'Ничего не найдено'));
    } else {
      const list = this.cat === 'Избранное' ? ['terminal', 'files', 'monitor', 'disks', 'net', 'system'].map(i => APPS[i]) : this.cat === 'Все' ? all : all.filter(a => a.cat === this.cat);
      g.replaceChildren(...list.map(a => item(a)));
    }
  }
  toggleLauncher() { this.launcherEl.classList.contains('open') ? this.closeLauncher() : this.openLauncher(); }
  openLauncher() { this.closeNotify(); this.launcherEl.classList.add('open'); this.lBtn.classList.add('open'); this.search.value = ''; this.cat = 'Избранное'; this.renderLauncher(); setTimeout(() => this.search.focus(), 20); }
  closeLauncher() { this.launcherEl.classList.remove('open'); this.lBtn.classList.remove('open'); }
  /* --- центр уведомлений (панель) --- */
  buildNotify() { this.notifyEl.addEventListener('mousedown', e => e.stopPropagation()); this.renderNotify(); }
  renderNotify() {
    const d = this, N = this.notifyEl;
    N.replaceChildren(h('div.tbar', h('b', 'Уведомления'), h('span.sp'), h('button.btn.sm', { onclick: () => { d.notes.forEach(n => n.unread = false); d.renderNotify(); d.renderTaskbar(); } }, 'Прочитать все'), h('button.btn.sm', { onclick: () => d.closeNotify() }, '✕')),
      h('div.scroll', { style: 'flex:1' }, d.notes.length ? d.notes.map(n => h('div.nitem' + (n.unread ? '.unread' : ''), { onclick: () => { n.unread = false; d.renderNotify(); d.renderTaskbar(); if (n.app) { d.closeNotify(); d.open(n.app); } } }, h('span.dot.' + ({ rd: 'err', am: 'off', cy: 'on' }[n.sev] || 'unk')), h('div', h('div', { style: 'font-weight:600' }, n.t), h('div.sub', n.d), h('div.mono.sub', { style: 'font-size:10.5px' }, n.time + ' · ' + d.srv.name)))) : h('div.empty', 'Уведомлений нет')),
      h('div.tbar', h('span.sub', 'Не беспокоить'), h('button.tog', { onclick: e => { e.currentTarget.classList.toggle('on'); } }), h('span.sp'), h('button.btn.sm', { onclick: () => { d.closeNotify(); d.open('notify'); } }, 'Правила и журнал…')));
  }
  toggleNotify() { this.notifyEl.classList.contains('open') ? this.closeNotify() : (this.closeLauncher(), this.renderNotify(), this.notifyEl.classList.add('open')); }
  closeNotify() { this.notifyEl.classList.remove('open'); }
  notify(sev, t, dsc, app) { this.notes.unshift({ sev, t, d: dsc, app, time: hhmm(), unread: true }); this.renderNotify(); this.renderTaskbar(); this.emit('notify'); toast(t); }
  log(action, res = 'ok') { this.audit.unshift({ time: hhmmss(), action, res }); this.emit('audit'); }
  /* --- обзор рабочих столов --- */
  toggleOverview() {
    const o = this.ovlEl; if (o.classList.contains('open')) return o.classList.remove('open');
    this.closeLauncher(); o.replaceChildren(h('h3', { style: 'margin:0 8px 10px' }, 'Рабочие столы'), h('div.tile', { onclick: () => { o.classList.remove('open'); showHome(); } }, h('b', 'Дом'), h('span.sub', 'Список серверов')),
      ...[...Desks.values()].map(x => h('div.tile', { onclick: () => { o.classList.remove('open'); activateTab(x.srv.id); } }, h('div.row', dot(x.srv.status), h('b', x.srv.name), h('span.sp'), h('span.chip.cy', x.wins.length + ' окон')), h('div.mono.sub', 'CPU ' + Math.round(x.m.cpu[59]) + '% · RAM ' + Math.round(x.m.ram[59]) + '%'), h('div.mono.sub', x.wins.map(w => w.app.name).join(' · ') || 'нет окон'), h('button.btn.sm', { onclick: e => { e.stopPropagation(); closeDeskTab(x.srv.id); o.classList.remove('open'); } }, 'Закрыть стол'))),
      h('button.btn.p', { style: 'margin:8px', onclick: () => { o.classList.remove('open'); showHome(); } }, '+ Открыть другой сервер'));
    o.classList.add('open');
  }
  /* --- окна --- */
  open(id, opts = {}) {
    const app = APPS[id]; if (!app) return;
    if (this.state === 'lost' && !opts.force) { /* окна можно открывать, данные «заморожены» */ }
    const ex = this.wins.filter(w => w.app.id === id);
    if (app.single && ex.length && !opts.fresh) { const w = ex[0]; w.focus(); if (opts.tab && w.api && w.api.tab) w.api.tab(opts.tab, opts); if (opts.path && w.api && w.api.cd) w.api.cd(opts.path); return w; }
    const w = new Win(this, app, opts); this.wins.push(w); this.recent.unshift(app.name); this.renderTaskbar(); if (opts.tab && w.api && w.api.tab) w.api.tab(opts.tab, opts); return w;
  }
  blurAll() { this.wins.forEach(w => { w.active = false; w.el.classList.remove('active'); }); this.renderTaskbar(); }
  showDesktop() { const any = this.wins.some(w => !w.min); this.wins.forEach(w => any ? w.minimize(true) : w.restore()); this.renderTaskbar(); }
  area() { const r = this.winsEl.getBoundingClientRect(); return { w: r.width, h: r.height, l: r.left, t: r.top }; }
  destroy() { clearInterval(this.timer); clearInterval(this.rc); this.wins.slice().forEach(w => w.close(true)); this.el.remove(); }
}

class Win {
  constructor(desk, app, opts) {
    this.desk = desk; this.app = app; this.min = false; this.max = false; this.active = false; this.cleanup = [];
    const A = desk.area(), W = Math.min(app.w || 760, A.w - 20), H = Math.min(app.h || 520, A.h - 20), n = desk.cascade++ % 8;
    this.el = h('div.win.anim', { style: { left: clamp(60 + n * 34 + (A.w - W) / 8, 0, A.w - W) + 'px', top: clamp(30 + n * 30, 0, A.h - H) + 'px', width: W + 'px', height: H + 'px', zIndex: ++ZTOP } });
    this.titleEl = h('span.t', app.name);
    this.subEl = h('span.s', '· ' + desk.srv.name);
    this.bar = h('div.wtitle', ico(app, 20), this.titleEl, this.subEl,
      h('div.wb', h('button', { title: 'Свернуть', onclick: e => { e.stopPropagation(); this.minimize(); } }, '–'), h('button', { title: 'Развернуть', onclick: e => { e.stopPropagation(); this.toggleMax(); } }, '□'), h('button.cl', { title: 'Закрыть', onclick: e => { e.stopPropagation(); this.close(); } }, '✕')));
    this.body = h('div.wbody');
    this.el.append(this.bar, this.body, ...['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'].map(d => h('div.rs.' + d, { onpointerdown: e => this.startResize(e, d) })));
    desk.winsEl.append(this.el);
    this.el.addEventListener('pointerdown', () => { if (!this.active) this.focus(); }, true);
    this.bar.addEventListener('pointerdown', e => { if (e.target.closest('.wb')) return; this.startDrag(e); });
    this.bar.addEventListener('dblclick', e => { if (!e.target.closest('.wb')) this.toggleMax(); });
    setTimeout(() => this.el.classList.remove('anim'), 200);
    try { this.api = app.build(this, desk, opts) || {}; } catch (e) { console.error(e); window.__errs && window.__errs.push(app.id + ': ' + (e.stack || e)); this.body.append(h('div.empty.rd', 'Ошибка: ' + e.message)); }
    this.focus();
  }
  setTitle(t, sub) { this.titleEl.textContent = t; if (sub != null) this.subEl.textContent = sub; }
  focus() { const d = this.desk; if (this.min) this.restore(); d.wins.forEach(w => { w.active = false; w.el.classList.remove('active'); }); this.active = true; this.el.classList.add('active'); this.el.style.zIndex = ++ZTOP; d.renderTaskbar(); d.closeLauncher(); this.body.dispatchEvent(new CustomEvent('wfocus')); }
  minimize(quiet) { this.min = true; this.active = false; this.el.classList.add('min', 'anim'); this.el.classList.remove('active'); if (!quiet) this.desk.renderTaskbar(); }
  restore() { this.min = false; this.el.classList.remove('min'); this.focus(); }
  toggleMax() {
    this.el.classList.add('anim'); const A = this.desk.area();
    if (this.max) { Object.assign(this.el.style, this.prev); this.max = false; }
    else { this.prev = { left: this.el.style.left, top: this.el.style.top, width: this.el.style.width, height: this.el.style.height }; Object.assign(this.el.style, { left: '0px', top: '0px', width: A.w + 'px', height: A.h + 'px' }); this.max = true; }
    setTimeout(() => { this.el.classList.remove('anim'); this.body.dispatchEvent(new CustomEvent('wresize')); }, 180);
  }
  close(silent) {
    const d = this.desk; if (this.api && this.api.beforeClose && !silent && this.api.beforeClose(() => this.close(true))) return;
    this.cleanup.forEach(f => f()); this.el.remove(); d.wins = d.wins.filter(w => w !== this);
    if (!silent) { const top = d.wins.filter(w => !w.min).sort((a, b) => b.el.style.zIndex - a.el.style.zIndex)[0]; top ? top.focus() : d.renderTaskbar(); }
  }
  snapRect(x, y, A) {
    const e = 10, L = x < e, R = x > A.w - e, T = y < e, Bt = y > A.h - e - 4;
    if (T && !L && !R) return { l: 0, t: 0, w: A.w, h: A.h, max: true };
    if (L && T) return { l: 0, t: 0, w: A.w / 2, h: A.h / 2 }; if (R && T) return { l: A.w / 2, t: 0, w: A.w / 2, h: A.h / 2 };
    if (L && Bt) return { l: 0, t: A.h / 2, w: A.w / 2, h: A.h / 2 }; if (R && Bt) return { l: A.w / 2, t: A.h / 2, w: A.w / 2, h: A.h / 2 };
    if (L) return { l: 0, t: 0, w: A.w / 2, h: A.h }; if (R) return { l: A.w / 2, t: 0, w: A.w / 2, h: A.h };
    return null;
  }
  startDrag(e) {
    if (e.button !== 0) return; const d = this.desk, A = d.area(), el = this.el; let moved = false, snap = null;
    const sx = e.clientX, sy = e.clientY; let ox = el.offsetLeft, oy = el.offsetTop;
    const mv = ev => {
      if (!moved && Math.abs(ev.clientX - sx) + Math.abs(ev.clientY - sy) < 4) return;
      if (!moved) { moved = true; el.classList.add('dragging'); if (this.max) { const rw = parseFloat(this.prev.width), rel = (sx - A.l) / A.w; Object.assign(el.style, this.prev); this.max = false; ox = clamp(sx - A.l - rw * rel, 0, A.w - rw); oy = 0; el.style.left = ox + 'px'; } }
      const px = ev.clientX - A.l, py = ev.clientY - A.t;
      el.style.left = (ox + ev.clientX - sx) + 'px'; el.style.top = clamp(oy + ev.clientY - sy, 0, A.h - 34) + 'px';
      snap = this.snapRect(px, py, A); const s = d.snapEl;
      if (snap) { Object.assign(s.style, { display: 'block', left: snap.l + 'px', top: snap.t + 'px', width: snap.w + 'px', height: snap.h + 'px' }); } else s.style.display = 'none';
    };
    const up = () => { document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); el.classList.remove('dragging'); d.snapEl.style.display = 'none';
      if (snap) { this.prev = this.prev && this.max ? this.prev : { left: el.style.left, top: el.style.top, width: el.style.width, height: el.style.height }; if (!snap.max) this.prev = { left: ox + 'px', top: oy + 'px', width: el.style.width, height: el.style.height };
        el.classList.add('anim'); Object.assign(el.style, { left: snap.l + 'px', top: snap.t + 'px', width: snap.w + 'px', height: snap.h + 'px' }); this.max = true; setTimeout(() => { el.classList.remove('anim'); this.body.dispatchEvent(new CustomEvent('wresize')); }, 180); } };
    document.addEventListener('pointermove', mv); document.addEventListener('pointerup', up);
  }
  startResize(e, dir) {
    e.preventDefault(); e.stopPropagation(); this.focus(); const el = this.el, A = this.desk.area(); const s = { x: e.clientX, y: e.clientY, l: el.offsetLeft, t: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight };
    const mv = ev => { const dx = ev.clientX - s.x, dy = ev.clientY - s.y; let { l, t, w, h: hh } = s;
      if (dir.includes('e')) w = Math.max(300, s.w + dx); if (dir.includes('s')) hh = Math.max(180, s.h + dy);
      if (dir.includes('w')) { w = Math.max(300, s.w - dx); l = s.l + s.w - w; } if (dir.includes('n')) { hh = Math.max(180, s.h - dy); t = Math.max(0, s.t + s.h - hh); hh = s.t + s.h - t; }
      Object.assign(el.style, { left: l + 'px', top: t + 'px', width: w + 'px', height: hh + 'px' }); this.max = false; this.body.dispatchEvent(new CustomEvent('wresize')); };
    const up = () => { document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); };
    document.addEventListener('pointermove', mv); document.addEventListener('pointerup', up);
  }
}
