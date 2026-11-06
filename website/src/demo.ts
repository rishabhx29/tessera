/**
 * Demo recording configuration.
 *
 * The video at /media/demo.mp4 is currently an honest placeholder slate: it
 * says the recording is being made and contains no app footage. When the real
 * recording is dropped in (same path, same poster and captions paths), set
 * DEMO_IS_PLACEHOLDER to false so the "Placeholder slate" badge and the note
 * under the player disappear.
 *
 * Drop-in instructions live in website/README.md.
 */
export const DEMO_IS_PLACEHOLDER = true;

export const DEMO_VIDEO = {
  src: '/media/demo.mp4',
  poster: '/media/demo-poster.png',
  captions: '/media/demo.en.vtt',
  captionsLabel: 'English',
} as const;
