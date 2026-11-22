// draw returns a delay in milliseconds on the rAF clock, or undefined when idle.
export function frameScheduler(draw) {
  let queued = false;
  let timer;
  function schedule() {
    clearTimeout(timer);
    if (queued) return;
    queued = true;
    requestAnimationFrame(time => {
      queued = false;
      const delay = draw(time);
      if (queued || delay === undefined) return;
      const remaining = time + delay - performance.now();
      if (remaining <= 0) schedule();
      else timer = setTimeout(schedule, Math.min(Math.ceil(remaining), 2147483647));
    });
  }
  return schedule;
}
