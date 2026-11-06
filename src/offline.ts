export async function setupOffline(): Promise<void> {
  if (!import.meta.env.PROD || !('serviceWorker' in navigator)) return;
  const button = document.querySelector<HTMLButtonElement>('#save-offline')!;
  const status = document.querySelector<HTMLElement>('#offline-status')!;
  const observe = (worker: ServiceWorker) => {
    const render = () => {
      const failed = worker.state === 'redundant';
      const ready = ['activated', 'activating', 'installed'].includes(worker.state);
      button.disabled = !failed;
      button.hidden = ready;
      status.textContent = ready ? 'Ready for offline play on this device.'
        : failed ? 'Download interrupted. Check your connection and free space, then try again.'
        : 'Saving game for offline play…';
    };
    worker.addEventListener('statechange', render);
    render();
  };
  navigator.serviceWorker.addEventListener('message', event => {
    if (event.data?.type === 'offline-progress') status.textContent = `Saving game for offline play… ${event.data.percent}%`;
  });
  const watch = (registration: ServiceWorkerRegistration) => {
    registration.addEventListener('updatefound', () => {
      if (registration.installing) observe(registration.installing);
    });
    const worker = registration.installing ?? registration.waiting ?? registration.active;
    if (worker) observe(worker);
  };
  button.addEventListener('click', async () => {
    button.disabled = true;
    try {
      watch(await navigator.serviceWorker.register('/sw.js'));
    } catch {
      status.textContent = 'Could not start the download. Reconnect and try again.';
      button.disabled = false;
    }
  });
  button.hidden = false;
  const existing = await navigator.serviceWorker.getRegistration();
  if (existing) watch(existing);
}
