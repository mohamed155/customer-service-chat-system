import { isChunkLoadError } from './chunk-load-error';

describe('isChunkLoadError', () => {
  it.each([
    [
      'Chrome and Edge',
      new TypeError('Failed to fetch dynamically imported module: https://x/y.js'),
    ],
    ['Firefox', new TypeError('error loading dynamically imported module: https://x/y.js')],
    ['Safari', new TypeError('Importing a module script failed.')],
    ['a webpack-style loader', new Error('Loading chunk 12 failed.')],
    ['a CSS chunk', new Error('Loading CSS chunk 3 failed.')],
    ['a preload failure', new Error('Unable to preload CSS for /styles.css')],
  ])('recognises the message of %s', (_browser, error) => {
    expect(isChunkLoadError(error)).toBe(true);
  });

  it('recognises the ChunkLoadError name whatever the message', () => {
    const error = new Error('something else');
    error.name = 'ChunkLoadError';
    expect(isChunkLoadError(error)).toBe(true);
  });

  it.each([
    ['another error', new Error('Cannot read properties of undefined')],
    ['a failed API call', new TypeError('Failed to fetch')],
    ['a string', 'Failed to fetch dynamically imported module'],
    ['null', null],
    ['undefined', undefined],
  ])('ignores %s', (_what, error) => {
    expect(isChunkLoadError(error)).toBe(false);
  });
});
