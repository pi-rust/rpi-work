const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');

const url = process.env.WORK_UI_URL || 'http://127.0.0.1:4180/';
const storageKey = 'rpi-work-resources-v1';
const manifest = (id, name) => ({
  schemaVersion: 1, kind: 'rpi-work-design-package', id, name,
  version: '1.0.0', author: 'Test', description: 'Isolated browser fixture',
  resources: { skills: [], tools: [], prompts: [] },
});
const resource = (id, name, design_sample) => ({
  id, name, design_sample, description: 'Isolated browser fixture',
  source: 'Test', path: 'skills/check/SKILL.md', content: '# Check', acquired: true,
});

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const results = [], errors = [];
  const pass = name => { results.push(name); console.log('PASS', name); };
  async function openPage(context) {
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(url);
    await page.locator('.sidebar').waitFor();
    return page;
  }
  async function openLibrary(page, name) {
    await page.locator('.sidebar').getByRole('button', { name, exact: true }).click();
    await page.locator('.resources-page').waitFor();
  }
  try {
    const fresh = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    const page = await openPage(fresh);
    for (const name of ['Packages', '技能库', '扩展']) {
      await openLibrary(page, name);
      assert.equal(await page.locator('.resource-card').count(), 0);
      await page.locator('.resources-page .resource-count').filter({ hasText: '0 项' }).waitFor({ state: 'attached' });
    }
    await page.reload();
    await openLibrary(page, 'Packages');
    assert.equal(await page.locator('.resource-card').count(), 0);
    pass('fresh launch and reload do not inject sample packages, skills or extensions');
    await fresh.close();

    const legacy = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    const stored = {
      version: 2,
      packages: [
        { manifest: manifest('test.sample', 'Sample Package'), design_sample: true },
        { manifest: manifest('test.user', 'My Package'), design_sample: false },
      ],
      skills: [resource('sample-skill', 'Sample Skill', true), resource('my-skill', 'My Skill', false)],
      extensions: [resource('sample-extension', 'Sample Extension', true), resource('my-extension', 'My Extension', false)],
      sessions: [{ session_id: 'node/session', instructions: 'Keep my settings', revision: 7,
        package_pins: [{ id: 'test.user', version: '1.0.0' }], skills: ['my-skill'],
        disabled_tools: ['bash'], disabled_skills: [], disabled_prompts: [] }],
    };
    await legacy.addInitScript(({ key, value }) => {
      if (!localStorage.getItem('test-resource-seeded')) {
        localStorage.setItem(key, JSON.stringify(value));
        localStorage.setItem('test-resource-seeded', '1');
      }
    }, { key: storageKey, value: stored });
    const migrated = await openPage(legacy);
    for (const [name, title] of [['Packages', 'My Package'], ['技能库', 'My Skill'], ['扩展', 'My Extension']]) {
      await openLibrary(migrated, name);
      await migrated.locator('.resource-card').filter({ hasText: title }).waitFor();
      assert.equal(await migrated.locator('.resource-card').count(), 1);
      assert.doesNotMatch(await migrated.locator('.resources-page').innerText(), /Sample Package|Sample Skill|Sample Extension/);
    }
    await migrated.waitForFunction(key => {
      const value = JSON.parse(localStorage.getItem(key));
      return ['packages', 'skills', 'extensions'].every(field => value[field].length === 1);
    }, storageKey);
    const saved = await migrated.evaluate(key => JSON.parse(localStorage.getItem(key)), storageKey);
    assert.deepEqual(saved.sessions, stored.sessions);
    for (const field of ['packages', 'skills', 'extensions']) assert.equal(saved[field][0].design_sample, false);
    pass('old cached samples are removed while user resources and session settings survive');
    await migrated.reload();
    await openLibrary(migrated, 'Packages');
    await migrated.locator('.resource-card').filter({ hasText: 'My Package' }).waitFor();
    assert.equal(await migrated.locator('.resource-card').count(), 1);
    pass('migration persists and samples do not return after reload');
    await legacy.close();
    assert.deepEqual(errors, []);
    pass('no application JavaScript errors');
  } finally {
    fs.mkdirSync('artifacts', { recursive: true });
    fs.writeFileSync('artifacts/ui-resource-cleanup-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
