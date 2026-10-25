import { parseHex } from './color.ts';

/**
 * Extracts dominant vibrant accent color from an image URL
 * using an off-screen 64x64 canvas and saturation-weighted hue binning.
 */
export async function extractDominantAccent(mediaUrl: string, _type: 'image' | 'video' = 'image'): Promise<string> {
  return new Promise(resolve => {
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d');
    if (!ctx) return resolve('#ffd43b');

    const processCanvas = () => {
      try {
        const imgData = ctx.getImageData(0, 0, 64, 64).data;
        const bins: { r: number; g: number; b: number; count: number; weight: number }[] = Array.from({ length: 12 }, () => ({
          r: 0, g: 0, b: 0, count: 0, weight: 0
        }));

        for (let i = 0; i < imgData.length; i += 4) {
          const r = imgData[i];
          const g = imgData[i + 1];
          const b = imgData[i + 2];
          const a = imgData[i + 3];
          if (a < 128) continue;

          const rNorm = r / 255, gNorm = g / 255, bNorm = b / 255;
          const max = Math.max(rNorm, gNorm, bNorm);
          const min = Math.min(rNorm, gNorm, bNorm);
          const lum = (max + min) / 2;

          // Ignore extreme darks and extreme lights
          if (lum < 0.15 || lum > 0.85) continue;

          const d = max - min;
          const sat = lum > 0.5 ? d / (2 - max - min) : d / (max + min);
          // Ignore washed-out grays
          if (sat < 0.20) continue;

          let hue = 0;
          if (max === rNorm) {
            hue = ((gNorm - bNorm) / d + (gNorm < bNorm ? 6 : 0)) / 6;
          } else if (max === gNorm) {
            hue = ((bNorm - rNorm) / d + 2) / 6;
          } else {
            hue = ((rNorm - gNorm) / d + 4) / 6;
          }

          const binIndex = Math.min(11, Math.floor(hue * 12));
          const weight = sat * (1 - Math.abs(lum - 0.5) * 1.2);
          const bin = bins[binIndex];
          bin.r += r * weight;
          bin.g += g * weight;
          bin.b += b * weight;
          bin.weight += weight;
          bin.count++;
        }

        let bestBin = bins[0];
        for (const bin of bins) {
          if (bin.weight > bestBin.weight) {
            bestBin = bin;
          }
        }

        if (bestBin.weight > 0) {
          const r = Math.round(bestBin.r / bestBin.weight);
          const g = Math.round(bestBin.g / bestBin.weight);
          const b = Math.round(bestBin.b / bestBin.weight);
          const hex = `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
          if (parseHex(hex)) return resolve(hex);
        }
      } catch (e) {
        console.warn('Accent extraction failed:', e);
      }
      resolve('#ffd43b');
    };

    const isRemote = (mediaUrl.startsWith('http://') || mediaUrl.startsWith('https://')) &&
      !mediaUrl.includes('asset.localhost') &&
      !mediaUrl.includes('127.0.0.1') &&
      !mediaUrl.includes('localhost');

    const img = new Image();
    if (isRemote) img.crossOrigin = 'anonymous';
    img.onload = () => {
      try {
        ctx.drawImage(img, 0, 0, 64, 64);
        processCanvas();
      } catch {
        resolve('#ffd43b');
      }
    };
    img.onerror = () => resolve('#ffd43b');
    img.src = mediaUrl;
    setTimeout(() => resolve('#ffd43b'), 2500);
  });
}
