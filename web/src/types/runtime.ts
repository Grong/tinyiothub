export type PlaybackProtocol = 'mjpeg' | 'hls';

export interface PlaybackDescriptor {
  protocol: PlaybackProtocol;
  url: string;
  expiresAt: string;
}
