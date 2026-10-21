// Дымовой тест рабочего стола в браузере с замоканным Tauri. Запуск: npm run build && npx vite preview --port 4173 & PLAYWRIGHT_CORE=<путь> node scripts/ui-smoke.cjs
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const servers = [
  { id: 's1', name: 'alpha', host: 'a.example', port: 22, username: 'root', group: 'prod', auth: { type: 'agent' }, enhanced_shell: false },
  { id: 's2', name: 'beta', host: 'b.example', port: 22, username: 'dev', group: null, auth: { type: 'agent' }, enhanced_shell: false },
];
(async () => {
  const exe = require('os').homedir() + '/.cache/ms-playwright/chromium-1248/chrome-linux/chrome';
  const browser = await chromium.launch({ executablePath: require('fs').existsSync(exe) ? exe : '/usr/bin/google-chrome', args: ['--no-sandbox'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  await page.addInitScript((servers) => {
    window.__calls = [];
    window.__TAURI_INTERNALS__ = {
      transformCallback: (cb) => { const id = Math.floor(Math.random() * 1e9); window['_' + id] = cb; return id; },
      invoke: async (cmd, args) => {
        window.__calls.push(cmd);
        if (cmd === 'list_servers') return servers;
        if (cmd === 'connection_status') return [];
        if (cmd === 'terminal_open') return { terminal_id: 't' + Math.random(), shell: 'default', install_hint: null };
        return null;
      },
    };
  }, servers);
  const ok = (c, m) => { if (!c) { console.log('FAIL', m); process.exitCode = 1; } else console.log('ok  ', m); };
  await page.goto('http://localhost:4173/');
  await page.waitForSelector('.window');
  ok(await page.locator('.window').count() === 1, 'при первом запуске открыто окно «Серверы»');
  // без выбранного сервера приложение не открывается, показывается подсказка
  await page.dblclick('.dicon:has-text("Терминал")');
  ok(await page.locator('.toast').count() === 1, 'подсказка «выберите сервер»');
  await page.selectOption('.active-server select', 's1');
  await page.dblclick('.dicon:has-text("Терминал")');
  await page.dblclick('.dicon:has-text("Терминал")');
  await page.waitForTimeout(300);
  ok(await page.locator('.window').count() === 3, 'два терминала одновременно + Серверы');
  ok(await page.locator('.xterm').count() === 2, 'xterm в обоих окнах');
  // перетаскивание
  const win = page.locator('.window').nth(2);
  const b0 = await win.boundingBox();
  await page.mouse.move(b0.x + 200, b0.y + 15); await page.mouse.down(); await page.mouse.move(b0.x + 300, b0.y + 80, { steps: 5 }); await page.mouse.up();
  const b1 = await win.boundingBox();
  ok(Math.abs(b1.x - b0.x - 100) < 3 && Math.abs(b1.y - b0.y - 65) < 3, `drag (${b1.x - b0.x}, ${b1.y - b0.y})`);
  // ресайз за правый нижний угол
  await page.mouse.move(b1.x + b1.width - 2, b1.y + b1.height - 2); await page.mouse.down(); await page.mouse.move(b1.x + b1.width + 78, b1.y + b1.height + 38, { steps: 5 }); await page.mouse.up();
  const b2 = await win.boundingBox();
  ok(Math.abs(b2.width - b1.width - 80) < 4 && Math.abs(b2.height - b1.height - 40) < 4, `resize (+${b2.width - b1.width}, +${b2.height - b1.height})`);
  // минимальный размер
  await page.mouse.move(b2.x + b2.width - 2, b2.y + b2.height - 2); await page.mouse.down(); await page.mouse.move(b2.x + 10, b2.y + 10, { steps: 5 }); await page.mouse.up();
  const b3 = await win.boundingBox();
  ok(b3.width >= 319 && b3.height >= 199, `минимальный размер (${b3.width}x${b3.height})`);
  // не уходит за пределы
  const t = await win.locator('.wtitle').boundingBox();
  await page.mouse.move(t.x + 100, t.y + 15); await page.mouse.down(); await page.mouse.move(2600, 2000, { steps: 5 }); await page.mouse.up();
  const b4 = await win.boundingBox();
  ok(b4.x < 1280 - 70 && b4.y < 756, `окно не ушло за стол (${b4.x},${b4.y})`);
  // snap влево
  const t2 = await win.locator('.wtitle').boundingBox();
  await page.mouse.move(t2.x + 60, t2.y + 15); await page.mouse.down(); await page.mouse.move(2, 300, { steps: 6 });
  ok(await page.locator('.snap-preview').count() === 1, 'превью snap');
  await page.mouse.up();
  const b5 = await win.boundingBox();
  ok(Math.abs(b5.width - 640) < 3 && b5.x < 2, `snap влево (${b5.x}, w=${b5.width})`);
  // двойной клик — развернуть, и обратно
  await win.locator('.wtitle').dblclick({ position: { x: 200, y: 10 } });
  const m = await win.boundingBox();
  ok(m.width >= 1279 && m.height >= 755 - 1, `развернуто (${m.width}x${m.height})`);
  await page.screenshot({ path: (process.env.SHOTS || '/tmp') + '/ui-max.png' });
  await win.locator('.wtitle').dblclick({ position: { x: 200, y: 10 } });
  const r = await win.boundingBox();
  ok(Math.abs(r.width - 320) < 3, 'восстановлено до размера до snap');
  // свернуть / панель задач
  await win.locator('.wbtn[title="Свернуть"]').click();
  ok(await win.isHidden(), 'свёрнуто');
  const tasks = page.locator('.task');
  ok(await tasks.count() === 3, 'три кнопки в панели задач');
  await tasks.nth(2).click();
  ok(await win.isVisible(), 'разворот из панели задач');
  // z-order
  await page.locator('.window').nth(0).locator('.wtitle').click({ position: { x: 150, y: 10 } });
  const z = await page.$$eval('.window', (w) => w.map((x) => +x.style.zIndex));
  ok(z[0] === Math.max(...z), 'фокус по клику поднимает окно');
  // Пуск
  await page.click('.start');
  await page.hover('.menu-item-wrap:has-text("Выполнить команду")');
  await page.click('.submenu >> text=beta');
  await page.waitForTimeout(200);
  ok(await page.locator('.window:has-text("Выполнить команду — beta")').count() === 1, 'Пуск → подменю по серверам открывает приложение для beta');
  // singleInstancePerServer: Серверы не дублируется
  await page.dblclick('.dicon:has-text("Серверы")');
  ok(await page.locator('.window').count() === 4, 'Серверы — одно окно');
  // перезагрузка: раскладка восстанавливается
  await page.waitForTimeout(500);
  const before = await page.$$eval('.window', (w) => w.map((x) => [x.style.left, x.style.top, x.style.width, x.style.height].join(',')));
  await page.reload(); await page.waitForSelector('.window'); await page.waitForTimeout(300);
  const after = await page.$$eval('.window', (w) => w.map((x) => [x.style.left, x.style.top, x.style.width, x.style.height].join(',')));
  ok(JSON.stringify(before) === JSON.stringify(after) && after.length === 4, `раскладка восстановлена (${after.length} окон)`);
  const opens = await page.evaluate(() => window.__calls.filter((c) => c === 'terminal_open').length);
  ok(opens === 2, 'терминалы переподключились при восстановлении (terminal_open ×' + opens + ')');
  // закрытие
  await page.locator('.window:has(.wname:has-text("Выполнить команду")) .wbtn.close').evaluate((b) => b.click());
  ok(await page.locator('.window').count() === 3, 'закрытие окна');
  await page.screenshot({ path: (process.env.SHOTS || '/tmp') + '/ui-final.png' });
  ok(errors.length === 0, 'нет ошибок консоли ' + errors.slice(0, 3).join(' | '));
  await browser.close();
})();
