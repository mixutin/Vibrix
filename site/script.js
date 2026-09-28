'use strict';

const REPOSITORY = 'https://github.com/mixutin/Vibrix';

function safeActivityUrl(candidate) {
  try {
    const url = new URL(candidate);
    if (url.origin === 'https://github.com' && !url.username && !url.password &&
        (url.pathname === '/mixutin/Vibrix' || url.pathname.startsWith('/mixutin/Vibrix/'))) {
      return url.href;
    }
  } catch (_) {
    // Invalid API data must not become a navigation target.
  }
  return REPOSITORY;
}

function dateText(value) {
  if (typeof value !== 'string') return 'Unavailable';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'Unavailable' :
    date.toLocaleDateString(undefined, {month: 'short', day: 'numeric'});
}

function counterText(value) {
  return Number.isSafeInteger(value) && value >= 0 ? String(value) : '—';
}

function activityItems(events) {
  if (!Array.isArray(events)) return [];
  return events.filter(event => event && typeof event === 'object').slice(0, 6).map(event => {
    const type = typeof event.type === 'string' ? event.type.replace(/Event$/, '').slice(0, 80) : 'Activity';
    const actor = typeof event.actor?.login === 'string' ? event.actor.login.slice(0, 100) : 'unknown contributor';
    return {
      date: dateText(event.created_at),
      text: `${type} by ${actor}`,
      url: safeActivityUrl(event.payload?.pull_request?.html_url || event.payload?.issue?.html_url || REPOSITORY),
      repository: 'mixutin/Vibrix'
    };
  });
}

function initializePresentation() {
  const reducedMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const output = document.querySelector('#terminal-lines');
  const lines = [
    ['[boot]', 'ExitBootServices complete', 300],
    ['[ ok ]', 'BootInfo v3 validated', 650],
    ['[ ok ]', 'GDT / IDT and native COM1', 1000],
    ['[ ok ]', 'ACPI and PCI discovery', 1350],
    ['[ ok ]', 'Timer IRQ delivery in QEMU', 1700],
    ['[ ok ]', 'Bounded PS/2 kernel console', 2050]
  ];
  if (output) {
    for (const [label, text, delay] of lines) {
      const appendLine = () => {
        const row = document.createElement('div');
        const marker = document.createElement('span');
        marker.className = label === '[boot]' ? 'dim' : 'ok';
        marker.textContent = label;
        row.append(marker, document.createTextNode(` ${text}`));
        output.appendChild(row);
      };
      if (reducedMotion) appendLine();
      else setTimeout(appendLine, delay);
    }
  }
  const typed = document.querySelector('#typed');
  if (typed) {
    typed.textContent = 'help';
    if (!reducedMotion) {
      const words = ['help', 'pci', 'mem', 'acpi', 'uptime'];
      let word = 0, index = 0, deleting = false;
      const type = () => {
        const current = words[word];
        typed.textContent = current.slice(0, index);
        if (!deleting && index < current.length) index++;
        else if (!deleting) { deleting = true; setTimeout(type, 1300); return; }
        else if (index > 0) index--;
        else { deleting = false; word = (word + 1) % words.length; }
        setTimeout(type, deleting ? 70 : 140);
      };
      setTimeout(type, 2500);
    }
  }
  const elements = document.querySelectorAll('.features article,.step,.layer');
  if (typeof IntersectionObserver === 'function' && !reducedMotion) {
    const observer = new IntersectionObserver(entries => {
      for (const entry of entries) {
        if (entry.isIntersecting) { entry.target.classList.add('seen'); observer.unobserve(entry.target); }
      }
    }, {threshold: 0.12});
    elements.forEach(element => observer.observe(element));
  } else {
    elements.forEach(element => element.classList.add('seen'));
  }
}

async function loadRepoActivity() {
  const feed = document.querySelector('#activity-feed');
  if (!feed) return;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 8000);
  try {
    const options = {signal: controller.signal, credentials: 'omit'};
    const [repoResponse, eventsResponse] = await Promise.all([
      fetch('https://api.github.com/repos/mixutin/Vibrix', options),
      fetch('https://api.github.com/repos/mixutin/Vibrix/events?per_page=8', options)
    ]);
    if (!repoResponse.ok || !eventsResponse.ok) throw new Error('GitHub API unavailable');
    const repo = await repoResponse.json();
    const events = await eventsResponse.json();
    if (!repo || typeof repo !== 'object' || !Array.isArray(events)) throw new Error('Unexpected API response');
    const counters = {'#stat-stars': counterText(repo.stargazers_count), '#stat-forks': counterText(repo.forks_count),
      '#stat-open': counterText(repo.open_issues_count), '#stat-pushed': dateText(repo.pushed_at)};
    for (const [selector, text] of Object.entries(counters)) {
      const element = document.querySelector(selector);
      if (element) element.textContent = text;
    }
    const items = activityItems(events);
    feed.replaceChildren();
    if (!items.length) feed.textContent = 'No recent public events returned by GitHub.';
    for (const item of items) {
      const row = document.createElement('div');
      row.className = 'feeditem';
      const time = document.createElement('time');
      time.textContent = item.date;
      const link = document.createElement('a');
      link.href = item.url;
      link.textContent = item.text;
      const repository = document.createElement('b');
      repository.textContent = item.repository;
      row.append(time, link, repository);
      feed.appendChild(row);
    }
  } catch (_) {
    feed.textContent = 'Live GitHub activity is unavailable. The source and verified status links still work.';
  } finally {
    clearTimeout(timer);
  }
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {safeActivityUrl, dateText, counterText, activityItems};
}
if (typeof document !== 'undefined') {
  initializePresentation();
  loadRepoActivity();
}
