import { icon, toast } from './dom.ts';

export interface VoiceVibeController {
  isRecording(): boolean;
  toggle(): Promise<void>;
  start(): Promise<void>;
  stop(): void;
  mount(container: HTMLElement, promptGetter: () => HTMLTextAreaElement | null): void;
  destroy(): void;
}

export function createVoiceVibe(): VoiceVibeController {
  let recording = false;
  let isStarting = false;
  let isVoiceActive = false; // Push-to-talk F8 state
  let isManualClick = false; // Click-to-toggle state
  let recognition: any = null;
  let audioContext: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let mediaStream: MediaStream | null = null;
  let mediaRecorder: MediaRecorder | null = null;
  let animFrame = 0;
  let waveCanvas: HTMLCanvasElement | null = null;
  let btn: HTMLButtonElement | null = null;
  let promptRef: () => HTMLTextAreaElement | null = () => null;

  function isSpeechSupported(): boolean {
    return typeof window !== 'undefined' && ('SpeechRecognition' in window || 'webkitSpeechRecognition' in window);
  }

  async function startAudioCapture(): Promise<void> {
    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      
      // If user already released F8 while getUserMedia was resolving, abort immediately
      if (!isVoiceActive && !isManualClick) {
        mediaStream.getTracks().forEach(track => track.stop());
        mediaStream = null;
        return;
      }

      // 1. Initialize persistent MediaRecorder for hardware audio lifecycle
      if (typeof MediaRecorder !== 'undefined') {
        try {
          mediaRecorder = new MediaRecorder(mediaStream);
          mediaRecorder.ondataavailable = () => {};
          mediaRecorder.onerror = () => {};
          mediaRecorder.start();
        } catch {
          mediaRecorder = null;
        }
      }

      // 2. Initialize AudioContext frequency visualizer
      const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
      if (AudioCtx) {
        audioContext = new AudioCtx();
        const source = audioContext.createMediaStreamSource(mediaStream);
        analyser = audioContext.createAnalyser();
        analyser.fftSize = 32;
        source.connect(analyser);
        drawWave();
      }
    } catch (err) {
      console.warn('Microphone stream access error:', err);
    }
  }

  function stopAudioCapture(): void {
    cancelAnimationFrame(animFrame);
    
    // Stop MediaRecorder cleanly
    if (mediaRecorder) {
      try {
        if (mediaRecorder.state !== 'inactive') {
          mediaRecorder.stop();
        }
      } catch {}
      mediaRecorder = null;
    }

    // Cleanly stop all tracks to release Windows microphone hardware
    if (mediaStream) {
      try {
        mediaStream.getTracks().forEach(track => {
          track.stop();
        });
      } catch {}
      mediaStream = null;
    }

    // Close AudioContext
    if (audioContext && audioContext.state !== 'closed') {
      try {
        audioContext.close().catch(() => {});
      } catch {}
      audioContext = null;
    }
    analyser = null;

    if (waveCanvas) {
      const ctx = waveCanvas.getContext('2d');
      if (ctx) ctx.clearRect(0, 0, waveCanvas.width, waveCanvas.height);
    }
  }

  function drawWave(): void {
    if (!recording || !waveCanvas || !analyser) return;
    const ctx = waveCanvas.getContext('2d');
    if (!ctx) return;

    const buffer = new Uint8Array(analyser.frequencyBinCount);
    analyser.getByteFrequencyData(buffer);

    const w = waveCanvas.width;
    const h = waveCanvas.height;
    ctx.clearRect(0, 0, w, h);

    const bars = 5;
    const barWidth = 6;
    const gap = 4;
    const startX = (w - (bars * barWidth + (bars - 1) * gap)) / 2;

    for (let i = 0; i < bars; i++) {
      const val = buffer[i * 2] || 0;
      const percent = Math.min(1, Math.max(0.15, val / 255));
      const barHeight = Math.max(3, percent * h);
      const x = startX + i * (barWidth + gap);
      const y = (h - barHeight) / 2;

      ctx.fillStyle = i % 2 === 0 ? 'var(--accent, #ffd633)' : '#ef4444';
      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, 3);
      ctx.fill();
    }

    animFrame = requestAnimationFrame(drawWave);
  }

  function insertTextAtCursor(textarea: HTMLTextAreaElement, text: string): void {
    const start = textarea.selectionStart ?? textarea.value.length;
    const end = textarea.selectionEnd ?? textarea.value.length;
    const before = textarea.value.substring(0, start);
    const after = textarea.value.substring(end);
    const spacePrefix = before.length > 0 && !before.endsWith(' ') && !before.endsWith('\n') ? ' ' : '';
    const insertion = spacePrefix + text;

    textarea.value = before + insertion + after;
    textarea.selectionStart = textarea.selectionEnd = start + insertion.length;
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
  }

  async function start(): Promise<void> {
    if (recording || isStarting) return;
    isStarting = true;

    try {
      if (isSpeechSupported()) {
        try {
          const Speech = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
          recognition = new Speech();
          recognition.continuous = true;
          recognition.interimResults = false;
          recognition.lang = navigator.language || 'en-US';

          recognition.onresult = (event: any) => {
            let finalTranscript = '';
            for (let i = event.resultIndex; i < event.results.length; ++i) {
              if (event.results[i].isFinal) {
                finalTranscript += event.results[i][0].transcript;
              }
            }
            if (finalTranscript.trim()) {
              const textarea = promptRef();
              if (textarea) {
                insertTextAtCursor(textarea, finalTranscript.trim());
              }
            }
          };

          // On Windows WebView2, transient speech errors must NOT terminate audio capture
          recognition.onerror = (e: any) => {
            console.warn('SpeechRecognition warning:', e?.error || e);
          };

          recognition.onend = () => {
            if (recording && recognition) {
              try {
                recognition.start();
              } catch {}
            }
          };

          recognition.start();
        } catch (e) {
          console.warn('SpeechRecognition initialization error:', e);
          recognition = null;
        }
      }

      await startAudioCapture();

      // Check if user cancelled while starting
      if (!isVoiceActive && !isManualClick) {
        stopAudioCapture();
        if (recognition) {
          try { recognition.stop(); } catch {}
          recognition = null;
        }
        return;
      }

      recording = true;
      if (btn) {
        btn.classList.add('recording');
        btn.setAttribute('aria-pressed', 'true');
        btn.title = 'Stop dictation (Click or release F8)';
      }
      if (waveCanvas) waveCanvas.parentElement?.removeAttribute('hidden');
    } catch (err) {
      console.warn('Voice start failed:', err);
      stop();
    } finally {
      isStarting = false;
    }
  }

  function stop(): void {
    isVoiceActive = false;
    isManualClick = false;
    if (!recording && !isStarting) return;
    recording = false;

    if (recognition) {
      try {
        recognition.stop();
      } catch {}
      recognition = null;
    }

    stopAudioCapture();

    if (btn) {
      btn.classList.remove('recording');
      btn.setAttribute('aria-pressed', 'false');
      btn.title = 'Dictate prompt (Hold F8 or click)';
    }
    if (waveCanvas) waveCanvas.parentElement?.setAttribute('hidden', '');
  }

  async function toggle(): Promise<void> {
    if (recording) {
      isManualClick = false;
      stop();
    } else {
      isManualClick = true;
      await start();
    }
  }

  // Push-to-talk keyboard listener (F8)
  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'F8') {
      e.preventDefault();
      // Guard against OS key auto-repeat and active state
      if (e.repeat || isVoiceActive) return;
      isVoiceActive = true;
      isManualClick = false;
      void start();
    }
  }

  function onKeyUp(e: KeyboardEvent): void {
    if (e.key === 'F8') {
      e.preventDefault();
      if (!isVoiceActive) return;
      isVoiceActive = false;
      stop();
    }
  }

  function onBlur(): void {
    if (isVoiceActive) {
      isVoiceActive = false;
      stop();
    }
  }

  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);
  window.addEventListener('blur', onBlur);

  return {
    isRecording: () => recording,
    toggle,
    start,
    stop,
    mount(container: HTMLElement, promptGetter: () => HTMLTextAreaElement | null) {
      promptRef = promptGetter;

      const wrap = document.createElement('div');
      wrap.style.display = 'inline-flex';
      wrap.style.alignItems = 'center';
      wrap.style.gap = '4px';

      wrap.innerHTML = `
        <button type="button" class="voice-vibe-btn" id="voice-dictate-btn" aria-label="Voice dictation" title="Dictate prompt (Hold F8 or click)">
          ${icon('mic')}
        </button>
        <div class="voice-wave-container" hidden>
          <canvas class="voice-wave-canvas" width="56" height="18"></canvas>
        </div>
      `;

      container.appendChild(wrap);
      btn = wrap.querySelector('#voice-dictate-btn');
      waveCanvas = wrap.querySelector('.voice-wave-canvas');

      btn?.addEventListener('click', e => {
        e.preventDefault();
        e.stopPropagation();
        void toggle();
      });
    },
    destroy() {
      stop();
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    },
  };
}
