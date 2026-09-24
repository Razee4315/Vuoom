// Entry for every page. Works without motion (nav, menu, videos, slider, stars); the motion
// layer is loaded only when the visitor has not asked for reduced motion.
import { initNav } from './nav';
import { initVideos } from './videos';
import { initBeforeAfter } from './beforeafter';
import { initStars } from './stars';
import { initKeycaps } from './keycaps';
import { initDownload } from './download';

const motion = document.documentElement.classList.contains('motion');

initNav();
initVideos(motion);
initStars();
initDownload();
document.querySelectorAll<HTMLElement>('[data-ba]').forEach(initBeforeAfter);
const keys = initKeycaps();

if (motion) {
  import('./motion').then((m) => m.initMotion(keys));
} else {
  document.documentElement.classList.remove('is-counting');
}
