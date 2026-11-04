export interface DisplaySettings {
  resolution: '240' | '480' | 'native';
  aspect: 'classic' | 'wide';
  look: 'classic' | 'modern';
  motion: 'smooth' | 'stepped';
  scaling: 'soft' | 'sharp';
}
export const CLASSIC: DisplaySettings = { resolution: '480', aspect: 'classic', look: 'classic', motion: 'stepped', scaling: 'soft' };
export const MODERN: DisplaySettings = { resolution: 'native', aspect: 'wide', look: 'modern', motion: 'smooth', scaling: 'soft' };
const choices = { resolution: ['240', '480', 'native'], aspect: ['classic', 'wide'], look: ['classic', 'modern'], motion: ['smooth', 'stepped'], scaling: ['soft', 'sharp'] };
export function readDisplaySettings(): DisplaySettings {
  const settings = { ...CLASSIC };
  try {
    const saved = JSON.parse(localStorage.getItem('jumpman-display') ?? '{}');
    for (const key of Object.keys(choices) as (keyof DisplaySettings)[]) {
      if (choices[key].includes(saved[key])) Object.assign(settings, { [key]: saved[key] });
    }
  } catch { /* Private browsing or stale preferences: use defaults. */ }
  return settings;
}
export function displaySize(settings: DisplaySettings, width: number, height: number, dpr: number) {
  const aspect = settings.aspect === 'classic' ? 4 / 3 : width / height;
  const cssHeight = Math.min(height, width / aspect);
  const cssWidth = cssHeight * aspect;
  const renderHeight = settings.resolution === 'native' ? Math.round(cssHeight * Math.min(dpr, 2)) : Number(settings.resolution);
  return { cssWidth, cssHeight, width: Math.round(renderHeight * aspect), height: renderHeight, aspect };
}
