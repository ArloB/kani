// @ts-check
// Cover image component — manga cover with no-cover fallback.

/**
 * Creates a cover image element. Returns a DOM node ready to insert.
 * @param {{ url?: string | null, alt?: string }} props
 * @returns {HTMLElement}
 */
export function createCoverImage({ url, alt = '' }) {
  const wrap = document.createElement('div');
  wrap.className = 'block relative w-full h-full overflow-hidden bg-surface-2';

  if (url) {
    const img = document.createElement('img');
    img.src = url;
    img.alt = alt;
    img.loading = 'lazy';
    img.decoding = 'async';
    img.className = 'absolute inset-0 w-full h-full object-cover';
    img.addEventListener('error', () => {
      img.remove();
      const fallback = document.createElement('div');
      fallback.className = 'absolute inset-0 flex items-center justify-center text-xs text-text-muted';
      fallback.textContent = 'No Cover';
      wrap.appendChild(fallback);
    });
    wrap.appendChild(img);
  } else {
    const fallback = document.createElement('div');
    fallback.className = 'absolute inset-0 flex items-center justify-center text-xs text-text-muted';
    fallback.textContent = 'No Cover';
    wrap.appendChild(fallback);
  }

  return wrap;
}
