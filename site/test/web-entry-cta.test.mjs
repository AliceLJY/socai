import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  LANGUAGE_OPTIONS,
  choosePreferredLanguage,
  resolvePageLanguage,
  withLanguage,
} from "../src/lib/languages.ts";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("homepage promotes the canonical Agent Web experience before desktop downloads", async () => {
  const [page, header, runtime, styles, showcaseStyles] = await Promise.all([
    read("src/pages/index.astro"),
    read("src/components/SiteHeader.astro"),
    read("src/scripts/site.ts"),
    read("src/styles/global.css"),
    read("src/styles/creator-showcase.css"),
  ]);

  assert.match(page, /const webDemoUrl = "https:\/\/agent\.socai\.io"/u);
  assert.doesNotMatch(page, /bookDemoUrl|book-demo/u);
  assert.match(page, /demo: "try the web app"/u);
  assert.match(page, /demo: "体验网页版"/u);
  assert.match(page, /desktop: "download the desktop app"/u);
  assert.match(page, /desktop: "下载桌面 App"/u);
  assert.match(page, /agentInstall: "install the Skill \/ CLI for your agent"/u);
  assert.match(page, /agentInstall: "给 Agent 安装 Skill \/ CLI"/u);
  const webEntry = page.indexOf('class="hero__entry hero__entry--primary"');
  const desktopEntry = page.indexOf('class="hero__entry-downloads"');
  const agentEntry = page.indexOf('class="hero__entry hero__entry--install"');
  assert.ok(
    webEntry >= 0 && webEntry < desktopEntry && desktopEntry < agentEntry,
    "Web, desktop, and agent install entries must stack in that order",
  );
  assert.doesNotMatch(page, /data-i18n="hero\.github"/u);
  assert.match(page, /data-platform-icon="macos"/u);
  assert.match(page, /data-platform-icon="windows"/u);
  assert.doesNotMatch(page, /<span>macOS<\/span>|<span>Windows<\/span>/u);
  assert.match(styles, /\.hero__entries[\s\S]*?flex-direction:\s*column/u);

  for (const language of ["zh", "en", "ja", "ko", "es", "fr", "de", "pt"]) {
    assert.match(page, new RegExp(`^    ${language}: \\{`, "mu"));
    assert.ok(LANGUAGE_OPTIONS.some(({ value }) => value === language));
  }
  assert.match(header, /data-language-select/u);
  assert.match(runtime, /choosePreferredLanguage/u);
  assert.match(runtime, /navigator\.languages/u);
  assert.match(runtime, /data-language-link/u);
  assert.match(showcaseStyles, /:not\(\[data-language="zh"\]\)/u);
});

test("language preference survives fallback pages and keeps outbound URLs aligned", () => {
  const languages = LANGUAGE_OPTIONS.map(({ value }) => value);
  const preferred = choosePreferredLanguage({
    search: "?campaign=launch&lang=ja",
    storedLanguage: "de",
    browserLanguages: ["fr-FR"],
  });

  assert.equal(preferred, "ja", "URL language must win over stored and browser preferences");
  assert.equal(resolvePageLanguage(preferred, languages), "ja");
  assert.equal(
    resolvePageLanguage(preferred, ["zh", "en"]),
    "en",
    "A page fallback must not change the preferred language",
  );

  const switched = withLanguage("https://socai.io/?campaign=launch#hero", "de");
  assert.equal(switched.searchParams.get("campaign"), "launch");
  assert.equal(switched.searchParams.get("lang"), "de");
  assert.equal(switched.hash, "#hero");
  assert.equal(withLanguage("https://agent.socai.io", "de").toString(), "https://agent.socai.io/?lang=de");
  assert.equal(withLanguage("https://agent.socai.io/book-demo", "de").toString(), "https://agent.socai.io/book-demo?lang=de");
});

test("contact page lists discord, wechat, github, then the demo form", async () => {
  const [page, vercel] = await Promise.all([
    read("src/pages/contact.astro"),
    read("vercel.json"),
  ]);
  const discord = page.indexOf('href={discordUrl}');
  const qr = page.indexOf('class="qr-card"');
  const github = page.indexOf('href={githubIssuesUrl}');
  const form = page.indexOf('data-book-form');
  assert.ok(discord >= 0 && discord < qr && qr < github && github < form);
  assert.match(page, /fill="#5865F2"/u);
  assert.match(page, /data-api-base="\/api\/server"/u);
  assert.match(page, /class="qr-card" data-zh-only/u);
  assert.match(page, /formTitle: "预约演示，或其他任何问题"/u);
  assert.match(page, /formEmail: "邮箱"/u);
  assert.doesNotMatch(page, /formPrivacy|这些信息仅用于回复本次演示预约/u);
  assert.match(page, /name="use_case"/u);
  assert.match(vercel, /"destination": "https:\/\/agent-api\.socai\.io\/:path\*"/u);
});
