() => {
  const selectors = ['main', 'header', 'h1', 'p', '.metrics', '.metrics article',
    '#resources', '.interactive', '#counter', '.chart', '.chart svg', 'footer'];
  return {
    url: location.href,
    userAgent: navigator.userAgent,
    viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
    resources: document.querySelector('#resources')?.textContent,
    clicks: document.querySelector('#count')?.textContent,
    imageLoaded: [...document.images].every(image => image.complete && image.naturalWidth > 0),
    elements: Object.fromEntries(selectors.map(selector => {
      const element = document.querySelector(selector);
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return [selector, {
        x: rect.x, y: rect.y, width: rect.width, height: rect.height,
        font: style.font, color: style.color, background: style.backgroundColor,
        display: style.display,
      }];
    })),
  };
}
