/** Accessible tab bars shared by the title menu and the pause dialog. */

/** Roving-focus target for an arrow/Home/End key press, or null when the key is not a tab key. */
export function nextTabIndex(count: number, current: number, key: string): number | null {
  if (count <= 0) return null;
  switch (key) {
    case 'ArrowRight': case 'ArrowDown': return (current + 1) % count;
    case 'ArrowLeft': case 'ArrowUp': return (current - 1 + count) % count;
    case 'Home': return 0;
    case 'End': return count - 1;
    default: return null;
  }
}

function tabsOf(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="tablist"] > [role="tab"]')];
}

/** Show the named panel inside `root` and mark its tab selected. */
export function selectMenuTab(root: HTMLElement, name: string, focus = false): void {
  const tabs = tabsOf(root);
  if (!tabs.some(tab => tab.dataset.tab === name)) return;
  for (const tab of tabs) {
    const on = tab.dataset.tab === name;
    tab.setAttribute('aria-selected', String(on));
    tab.tabIndex = on ? 0 : -1;
    if (on && focus) tab.focus();
  }
  for (const panel of root.querySelectorAll<HTMLElement>('[role="tabpanel"]')) panel.hidden = panel.dataset.panel !== name;
  root.dataset.activeTab = name;
}

export function initTabs(root: HTMLElement, initial = 'modes'): void {
  const tabs = tabsOf(root);
  tabs.forEach(tab => {
    tab.addEventListener('click', event => {
      event.stopPropagation();
      selectMenuTab(root, tab.dataset.tab ?? initial);
    });
    tab.addEventListener('keydown', event => {
      const next = nextTabIndex(tabs.length, tabs.indexOf(tab), event.key);
      if (next === null) return;
      event.preventDefault();
      event.stopPropagation();
      selectMenuTab(root, tabs[next].dataset.tab ?? initial, true);
    });
  });
  selectMenuTab(root, initial);
}
