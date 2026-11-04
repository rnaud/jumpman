import puppeteer from 'puppeteer-core';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL = process.env.URL || 'http://127.0.0.1:5173/?level=playground&character=procedural';

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: [
    '--no-sandbox',
    '--disable-setuid-sandbox',
    '--enable-unsafe-swiftshader',
    '--use-gl=angle',
    '--use-angle=swiftshader',
    '--window-size=1440,900',
  ],
  defaultViewport: { width: 1440, height: 900, deviceScaleFactor: 1 },
});

const page = await browser.newPage();
const errors = [];
const logs = [];
page.on('console', (msg) => {
  logs.push(`${msg.type()}: ${msg.text()}`);
  if (msg.type() === 'error') errors.push(msg.text());
});
page.on('pageerror', (err) => errors.push(`pageerror: ${err.message}`));

await page.goto(URL, { waitUntil: 'networkidle0', timeout: 60000 });
await new Promise((r) => setTimeout(r, 1500));
await page.screenshot({ path: 'screenshots/01-start.png' });

const before = await page.evaluate(() => window.__JUMPMAN__?.debugState);
console.log('before start:', JSON.stringify(before));

// Click start.
await page.click('#start-button');
await new Promise((r) => setTimeout(r, 700));
await page.screenshot({ path: 'screenshots/02-spawn.png' });

// Walk forward for 1.4 seconds.
await page.keyboard.down('w');
await new Promise((r) => setTimeout(r, 1400));
const walking = await page.evaluate(() => window.__JUMPMAN__?.debugState);
await page.screenshot({ path: 'screenshots/03-walking.png' });
await page.keyboard.up('w');

// Jump while stationary-ish.
await new Promise((r) => setTimeout(r, 300));
await page.keyboard.down('Space');
await new Promise((r) => setTimeout(r, 220));
const jumping = await page.evaluate(() => window.__JUMPMAN__?.debugState);
await page.screenshot({ path: 'screenshots/04-jump.png' });
await page.keyboard.up('Space');
await new Promise((r) => setTimeout(r, 1200));

const after = await page.evaluate(() => window.__JUMPMAN__?.debugState);
console.log('walking:', JSON.stringify(walking));
console.log('jumping:', JSON.stringify(jumping));
console.log('after:  ', JSON.stringify(after));
console.log('errors:', errors);
console.log('logs:', logs.slice(-10));

await browser.close();

if (errors.length) process.exit(1);
if (!before || before.started !== false) {
  console.error('Game did not initialise correctly');
  process.exit(1);
}
if (!walking || Math.abs(walking.pos[2] - before.pos[2]) < 10) {
  console.error('Mario did not move');
  process.exit(1);
}
if (!jumping || jumping.pos[1] <= walking.pos[1] + 2) {
  console.error('Mario did not jump');
  process.exit(1);
}
