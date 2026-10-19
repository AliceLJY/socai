import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';

const source = stripTypeScriptTypes(await readFile(new URL('../src/lib/signup-campaign.ts', import.meta.url), 'utf8'));
const { campaignCountdown, campaignCopy, SIGNUP_CAMPAIGN } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);

test('countdown uses the agreed Los Angeles cutoff and never goes negative', () => {
  const end = Date.parse(SIGNUP_CAMPAIGN.cutoff);
  assert.equal(campaignCountdown(end - 90061000), '01d 01h 01m 01s');
  assert.equal(campaignCountdown(end - 1), '00d 00h 00m 01s');
  assert.equal(campaignCountdown(end), null);
  assert.equal(campaignCountdown(end + 1000), null);
  assert.equal(SIGNUP_CAMPAIGN.credits, 300);
});

test('all supported languages advertise the same actual signup reward', () => {
  for (const lang of ['zh', 'en', 'ja', 'ko', 'es', 'fr', 'de', 'pt']) {
    const text = campaignCopy(lang);
    assert.ok(text.offer.includes('300'));
    assert.ok(text.invite && text.close && text.title);
  }
  assert.equal(campaignCopy('zh-CN').invite, '邀请活动');
  assert.equal(campaignCopy('unknown').offer, campaignCopy('en').offer);
});
