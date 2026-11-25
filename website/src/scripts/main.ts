// Page entry point: boots shared shell features everywhere and lazy-loads
// homepage interactive labs only where their DOM root exists.
import { initNav } from './nav';
import { initKeyboardGroups } from './a11y';
import { initVideos } from './videos';
import { initBeforeAfter } from './beforeafter';
import { initStars } from './stars';
import { initKeycaps } from './keycaps';
import { initDownload } from './download';

const motion = document.documentElement.classList.contains('motion');

initNav();
initKeyboardGroups();
initVideos(motion);
initStars();
initDownload();
document.querySelectorAll<HTMLElement>('[data-ba]').forEach(initBeforeAfter);
const keys = initKeycaps();

const heroStage = document.querySelector<HTMLElement>('[data-camera-playground]');
if (heroStage) {
  import('./camera-playground').then((m) => m.initCameraPlayground(heroStage, motion));
}

const screenRoom = document.querySelector<HTMLElement>('[data-screening-room]');
if (screenRoom) {
  import('./screening-room').then((m) => m.initScreeningRoom(screenRoom));
}

const studioFinisher = document.querySelector<HTMLElement>('[data-studio-finisher]');
if (studioFinisher) {
  import('./studio-finisher').then((m) => m.initStudioFinisher(studioFinisher));
}

const audioPanel = document.querySelector<HTMLElement>('[data-audio-panel]');
if (audioPanel) {
  import('./audio-panel').then((m) => m.initAudioPanel(audioPanel, motion));
}

const exportCalc = document.querySelector<HTMLElement>('[data-export-estimator]');
if (exportCalc) {
  import('./export-estimator').then((m) => m.initExportEstimator(exportCalc, motion));
}

if (motion) {
  if (document.querySelector('[data-reveal]')) {
    import('./motion').then((m) => m.initMotion());
  }
  const stage = document.querySelector<HTMLElement>('[data-stage]');
  if (stage) {
    import('./stage').then((m) =>
      m.initStage(stage, { onZoom: (zoomIn) => { if (zoomIn) keys.press(); } }).start(),
    );
  }
}
