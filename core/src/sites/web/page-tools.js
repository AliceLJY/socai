(() => {
  function searchResults({ query }) {
    const page = new URL(location.href);
    const text = document.body?.innerText || '';
    if (/unfortunately, bots use duckduckgo too|please complete the following challenge|select all squares containing a duck/i.test(text)) {
      return { reason: 'DuckDuckGo requires verification; search is unavailable.' };
    }
    if (page.hostname !== 'html.duckduckgo.com' || page.searchParams.get('q') !== query) {
      return { reason: 'Search page does not match the requested query.' };
    }
    const results = [];
    const seen = new Set();
    for (const link of document.querySelectorAll('a.result__a')) {
      let url;
      try {
        url = new URL(link.href, location.href);
        if ((url.hostname === 'duckduckgo.com' || url.hostname.endsWith('.duckduckgo.com')) && url.pathname.startsWith('/l/')) {
          url = new URL(url.searchParams.get('uddg') || '');
        }
      } catch { continue; }
      if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password || seen.has(url.href)) continue;
      if (url.hostname === 'duckduckgo.com' || url.hostname.endsWith('.duckduckgo.com')) continue;
      const title = (link.textContent || '').trim().slice(0, 500);
      if (!title) continue;
      seen.add(url.href);
      results.push({ title, url: url.href, snippet: (link.closest('.result')?.querySelector('.result__snippet')?.textContent || '').trim().slice(0, 1200) });
      if (results.length === 10) break;
    }
    if (!results.length) return { reason: 'No public search results are visible; the page may be empty, blocked or still loading.' };
    return { query, url: location.href, results };
  }
  window.SocaiWebPageTools = { searchResults };
})();
