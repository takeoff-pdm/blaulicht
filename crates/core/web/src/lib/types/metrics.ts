//
// Metrics
//

export interface SystemMetrics {
  // System
  loopSpeed: number;
  tickSpeed: number;
  // Audio.
  audio: AudioMetrics;
}

export interface AudioMetrics {
  volume: number;
  bass: number;
  bassAvg: number;
  bassAvgShort: number;
  bpm: number;
  beatVolume: number,
}