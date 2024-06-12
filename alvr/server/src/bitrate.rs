use crate::FfiDynamicEncoderParams;
use alvr_common::{SlidingWindowAverage, warn};
use alvr_events::{EventType, HeuristicStats, NominalBitrateStats};
use alvr_session::{
    settings_schema::Switch, BitrateAdaptiveFramerateConfig, BitrateConfig, BitrateMode,
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use rand::{distributions::Uniform, random};
use rand::{thread_rng, Rng};

const UPDATE_INTERVAL: Duration = Duration::from_secs(1);

pub struct BitrateManager {
    nominal_frame_interval: Duration,
    frame_interval_average: SlidingWindowAverage<Duration>,
    // note: why packet_sizes_bits_history is a queue and not a sliding average? Because some
    // network samples will be dropped but not any packet size sample
    packet_sizes_bits_history: VecDeque<(Duration, usize)>,
    encoder_latency_average: SlidingWindowAverage<Duration>,
    network_latency_average: SlidingWindowAverage<Duration>,
    bitrate_average: SlidingWindowAverage<f32>,
    decoder_latency_overstep_count: usize,
    last_frame_instant: Instant,
    last_update_instant: Instant,
    dynamic_max_bitrate: f32,
    previous_config: Option<BitrateConfig>,
    update_needed: bool,

    last_target_bitrate: f32,

    frame_interarrival_avg: f32,

    alt_network_latency_window: SlidingWindowAverage<Duration>,
    update_interval_setting: Duration,

    heur_stats: HeuristicStats, 
    // last_random_prob_heuristic: f32,
}
impl BitrateManager {
    pub fn new(max_history_size: usize, initial_framerate: f32) -> Self {
        Self {
            nominal_frame_interval: Duration::from_secs_f32(1. / initial_framerate),
            frame_interval_average: SlidingWindowAverage::new(
                Duration::from_millis(16),
                max_history_size,
            ),
            packet_sizes_bits_history: VecDeque::new(),
            encoder_latency_average: SlidingWindowAverage::new(
                Duration::from_millis(5),
                max_history_size,
            ),
            network_latency_average: SlidingWindowAverage::new(
                Duration::from_millis(5),
                max_history_size,
            ),
            bitrate_average: SlidingWindowAverage::new(30_000_000.0, max_history_size),
            decoder_latency_overstep_count: 0,
            last_frame_instant: Instant::now(),
            last_update_instant: Instant::now(),
            dynamic_max_bitrate: f32::MAX,
            previous_config: None,
            update_needed: true,

            last_target_bitrate: 30_000_000.0,

            frame_interarrival_avg: 0.011,
            alt_network_latency_window: SlidingWindowAverage::new(
                Duration::from_millis(5),
                max_history_size,
            ),
            update_interval_setting: UPDATE_INTERVAL,
            heur_stats: HeuristicStats{..Default::default()}, 
        }
    }

    // Note: This is used to calculate the framerate/frame interval. The frame present is the most
    // accurate event for this use.
    pub fn report_frame_present(&mut self, config: &Switch<BitrateAdaptiveFramerateConfig>) {
        let now = Instant::now();

        let interval = now - self.last_frame_instant;
        self.last_frame_instant = now;

        self.frame_interval_average.submit_sample(interval);

        if let Some(config) = config.as_option() {
            let interval_ratio =
                interval.as_secs_f32() / self.frame_interval_average.get_average().as_secs_f32();

            if interval_ratio > config.framerate_reset_threshold_multiplier
                || interval_ratio < 1.0 / config.framerate_reset_threshold_multiplier
            {
                // Clear most of the samples, keep some for stability
                self.frame_interval_average.retain(5);
                self.update_needed = true;
            }
        }
    }

    pub fn report_frame_encoded(
        &mut self,
        timestamp: Duration,
        encoder_latency: Duration,
        size_bytes: usize,
    ) {
        self.encoder_latency_average.submit_sample(encoder_latency);

        self.packet_sizes_bits_history
            .push_back((timestamp, size_bytes * 8));
    }

    // decoder_latency is used to learn a suitable maximum bitrate bound to avoid decoder runaway
    // latency

    pub fn report_network_rtt(&mut self, network_rtt: Duration) -> HeuristicStats{
        self.alt_network_latency_window.submit_sample(network_rtt);

        return self.heur_stats.clone()
    }

    pub fn report_frame_latencies(
        &mut self,
        config: &BitrateMode,
        timestamp: Duration,
        network_latency: Duration,
        decoder_latency: Duration,

        frame_interarrival_avg: f32,
    ) {
        if network_latency.is_zero() {
            return;
        }
        self.frame_interarrival_avg = frame_interarrival_avg;

        self.network_latency_average.submit_sample(network_latency);

        while let Some(&(timestamp_, size_bits)) = self.packet_sizes_bits_history.front() {
            if timestamp_ == timestamp {
                self.bitrate_average
                    .submit_sample(size_bits as f32 / network_latency.as_secs_f32());

                self.packet_sizes_bits_history.pop_front();

                break;
            } else {
                self.packet_sizes_bits_history.pop_front();
            }
        }

        if let BitrateMode::Adaptive {
            decoder_latency_limiter: Switch::Enabled(config),
            ..
        } = &config
        {
            if decoder_latency > Duration::from_millis(config.max_decoder_latency_ms) {
                self.decoder_latency_overstep_count += 1;

                if self.decoder_latency_overstep_count == config.latency_overstep_frames {
                    self.dynamic_max_bitrate =
                        f32::min(self.bitrate_average.get_average(), self.dynamic_max_bitrate)
                            * config.latency_overstep_multiplier;

                    self.update_needed = true;

                    self.decoder_latency_overstep_count = 0;
                }
            } else {
                self.decoder_latency_overstep_count = 0;
            }
        }
    }


    pub fn report_heuristic_event(&mut self, heur: HeuristicStats){
        alvr_events::send_event(EventType::HeuristicStats(
            heur.clone(),
        )); 
    }

    pub fn get_encoder_params(
        &mut self,
        config: &BitrateConfig,
    ) -> (FfiDynamicEncoderParams, Option<NominalBitrateStats>) {
        let now = Instant::now();

        if let BitrateMode::SimpleHeuristic {
            update_interval_heuristic,
            ..
        } = &config.mode
        {
            if let Switch::Enabled(time_update) = update_interval_heuristic {
                self.update_interval_setting = Duration::from_secs_f32(*time_update);
            } else {
                self.update_interval_setting = UPDATE_INTERVAL;
            }
        } else {
            self.update_interval_setting = UPDATE_INTERVAL;
        }

        if self
            .previous_config
            .as_ref()
            .map(|prev| config != prev)
            .unwrap_or(true)
        {
            self.previous_config = Some(config.clone());
            // Continue method. Always update bitrate in this case
        } else if !self.update_needed
            && (now < (self.last_update_instant + self.update_interval_setting)
                || matches!(config.mode, BitrateMode::ConstantMbps(_)))
        {
            return (
                FfiDynamicEncoderParams {
                    updated: 0,
                    bitrate_bps: 0,
                    framerate: 0.0,
                },
                None,
            );
        }

        self.last_update_instant = now;
        self.update_needed = false;

        let mut stats = NominalBitrateStats::default();

        let bitrate_bps = match &config.mode {
            BitrateMode::ConstantMbps(bitrate_mbps) => *bitrate_mbps as f32 * 1e6,
            BitrateMode::SimpleHeuristic {
                max_bitrate_mbps,
                min_bitrate_mbps,
                steps_mbps,
                threshold_random_uniform,
                multiplier_rtt_threshold,
                fps_threshold_multiplier,
                ..
            } => {
                //define generator and sample from uniform dist. for heuristic
                let mut rng = thread_rng();
                let uniform_dist = Uniform::new(0.0, 1.0);

                fn minmax_bitrate(
                    bitrate_bps: f32,
                    max_bitrate_mbps: &Switch<f32>,
                    min_bitrate_mbps: &Switch<f32>,
                ) -> f32 {
                    // local function to just minmax after every change from heuristic to avoid blot code
                    let mut bitrate = bitrate_bps;
                    if let Switch::Enabled(max) = max_bitrate_mbps {
                        let max = *max as f32 * 1e6;
                        bitrate = f32::min(bitrate, max);
                    }
                    if let Switch::Enabled(min) = min_bitrate_mbps {
                        let min = *min as f32 * 1e6;
                        bitrate = f32::max(bitrate, min);
                    }
                    bitrate
                }

                let initial_bitrate = self.last_target_bitrate;
                let mut bitrate_bps: f32 = initial_bitrate;
                
                let frame_interval = self.frame_interval_average.get_average();
                let framerate = 1.0 / frame_interval.as_secs_f32().min(1.0);
                let rtt_avg_heur = self.alt_network_latency_window.get_average().as_secs_f32();
                let fps_heur = 1.0 / self.frame_interarrival_avg;
                let random_prob = rng.sample(uniform_dist);
                
                if let Switch::Enabled(rtt_threshold_mult) = *multiplier_rtt_threshold {
                    if let Switch::Enabled(threshold_u) = *threshold_random_uniform {
                        if let Switch::Enabled(steps) = *steps_mbps {
                            if let Switch::Enabled(fps_mult) = *fps_threshold_multiplier {
                                let steps_bps = steps * 1E6;
                                
                                // Calculate thresholds
                                let threshold_fps = fps_mult * framerate;
                                let threshold_rtt = frame_interval.as_secs_f32() * rtt_threshold_mult;
                
                                if fps_heur >= threshold_fps {
                                    if rtt_avg_heur > threshold_rtt {
                                        if random_prob >= threshold_u {
                                            bitrate_bps -= steps_bps; // decrease bitrate by 1 step
                                        }
                                    } else {
                                        if random_prob <= threshold_u {
                                            bitrate_bps += steps_bps; // increase bitrate by 1 step
                                        }
                                    }
                                } else {
                                    bitrate_bps -= steps_bps; // decrease bitrate by 1 step
                                }
                
                                // Ensure bitrate is within allowed range
                                bitrate_bps = minmax_bitrate(bitrate_bps, max_bitrate_mbps, min_bitrate_mbps);
                
                                // Update heuristic stats
                                let heur_stats = HeuristicStats {
                                    frame_interval: frame_interval,
                                    framerate: framerate,
                                    steps_bps: steps_bps,
                
                                    fps_heur: fps_heur,
                                    rtt_avg_heur: rtt_avg_heur,
                                    random_prob: random_prob,
                
                                    threshold_fps: threshold_fps,
                                    threshold_rtt: threshold_rtt,
                                    threshold_u: threshold_u,
                                };
                                warn!("Heuristic Stats reported:  {:?}", heur_stats);
                                self.heur_stats = heur_stats.clone();
                            }
                        }
                    }
                }
                
                self.last_target_bitrate = bitrate_bps;
                if let Switch::Enabled(max) = max_bitrate_mbps {
                    let maxi = *max as f32 * 1e6;
                    stats.manual_max_bps = Some(maxi);
                }
                if let Switch::Enabled(min) = min_bitrate_mbps {
                    let mini = *min as f32 * 1e6;
                    stats.manual_min_bps = Some(mini);
                }
                bitrate_bps
            }
            BitrateMode::Adaptive {
                saturation_multiplier,
                max_bitrate_mbps,
                min_bitrate_mbps,
                max_network_latency_ms,
                encoder_latency_limiter,
                ..
            } => {
                let initial_bitrate_average_bps = self.bitrate_average.get_average();
                // let initial_bitrate_average_bps = self.last_target_bitrate;

                let mut bitrate_bps = initial_bitrate_average_bps * saturation_multiplier;
                stats.scaled_calculated_bps = Some(bitrate_bps);

                bitrate_bps = f32::min(bitrate_bps, self.dynamic_max_bitrate);
                stats.decoder_latency_limiter_bps = Some(self.dynamic_max_bitrate);

                if let Switch::Enabled(max_ms) = max_network_latency_ms {
                    let max = initial_bitrate_average_bps * (*max_ms as f32 / 1000.0)
                        / self.network_latency_average.get_average().as_secs_f32();
                    bitrate_bps = f32::min(bitrate_bps, max);

                    stats.network_latency_limiter_bps = Some(max);
                }

                if let Switch::Enabled(config) = encoder_latency_limiter {
                    let saturation = self.encoder_latency_average.get_average().as_secs_f32()
                        / self.nominal_frame_interval.as_secs_f32();
                    let max =
                        initial_bitrate_average_bps * config.max_saturation_multiplier / saturation;
                    stats.encoder_latency_limiter_bps = Some(max);

                    if saturation > config.max_saturation_multiplier {
                        // Note: this assumes linear relationship between bitrate and encoder
                        // latency but this may not be the case
                        bitrate_bps = f32::min(bitrate_bps, max);
                    }
                }

                if let Switch::Enabled(max) = max_bitrate_mbps {
                    let max = *max as f32 * 1e6;
                    bitrate_bps = f32::min(bitrate_bps, max);

                    stats.manual_max_bps = Some(max);
                }
                if let Switch::Enabled(min) = min_bitrate_mbps {
                    let min = *min as f32 * 1e6;
                    bitrate_bps = f32::max(bitrate_bps, min);

                    stats.manual_min_bps = Some(min);
                }

                bitrate_bps
            }
        };

        stats.requested_bps = bitrate_bps;

        let frame_interval = if config.adapt_to_framerate.enabled() {
            self.frame_interval_average.get_average()
        } else {
            self.nominal_frame_interval
        };
        self.last_target_bitrate = bitrate_bps;

        (
            FfiDynamicEncoderParams {
                updated: 1,
                bitrate_bps: bitrate_bps as u64,
                framerate: 1.0 / frame_interval.as_secs_f32().min(1.0),
            },
            Some(stats),
        )
    }
}