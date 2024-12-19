use alvr_common::{SlidingWindowAverage, SlidingWindowTimely, SlidingWindowWeighted, HEAD_ID};
use alvr_events::{
    EventType, GraphNetworkStatistics, GraphStatistics, NominalBitrateStats, StatisticsSummary,
};
use alvr_packets::{ClientStatistics, NetworkStatisticsPacket};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

const FULL_REPORT_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct HistoryFrame {
    target_timestamp: Duration,

    tracking_received: Instant,
    frame_present: Instant,
    frame_composed: Instant,
    frame_encoded: Instant,
    video_packet_bytes: usize,

    frame_index: i32,
    is_idr: bool,

    is_composed: bool,
    is_encoded: bool,
}

impl Default for HistoryFrame {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            target_timestamp: Duration::ZERO,

            tracking_received: now,
            frame_present: now,
            frame_composed: now,
            frame_encoded: now,
            video_packet_bytes: 0,

            frame_index: -1,
            is_idr: false,

            is_composed: false,
            is_encoded: false,
        }
    }
}

#[derive(Default, Clone)]
struct BatteryData {
    gauge_value: f32,
    is_plugged: bool,
}

pub struct StatisticsManager {
    history_buffer: VecDeque<HistoryFrame>,
    max_history_size: usize,

    last_full_report_instant: Instant,
    last_nominal_bitrate_stats: NominalBitrateStats,

    last_frame_present_instant: Instant,
    last_frame_present_interval: Duration,

    last_vsync_time: Instant,

    video_packets_total: usize,
    video_packets_partial_sum: usize,

    video_bytes_total: usize,
    video_bytes_partial_sum: usize,

    video_shards_sent_partial_sum: isize,
    video_shards_lost_partial_sum: isize,

    received_video_bytes_partial_sum: f32,

    frame_interarrival_partial_sum: f32,

    packets_dropped_total: usize,
    packets_dropped_partial_sum: usize,

    packets_skipped_total: usize,
    packets_skipped_partial_sum: usize,

    battery_gauges: HashMap<u64, BatteryData>,
    steamvr_pipeline_latency: Duration,

    // Latency metrics
    total_pipeline_latency_average: SlidingWindowAverage<Duration>,
    game_delay_average: SlidingWindowAverage<Duration>,
    server_compositor_average: SlidingWindowAverage<Duration>,
    encode_delay_average: SlidingWindowAverage<Duration>,
    network_delay_average: SlidingWindowAverage<Duration>,
    decode_delay_average: SlidingWindowAverage<Duration>,
    decoder_queue_delay_average: SlidingWindowAverage<Duration>,
    client_compositor_average: SlidingWindowAverage<Duration>,
    vsync_queue_delay_average: SlidingWindowAverage<Duration>,

    frame_interval: Duration,

    frame_interval_average: SlidingWindowAverage<Duration>,
    client_frame_interval_average: SlidingWindowAverage<Duration>,

    frame_interarrival_average: SlidingWindowAverage<f32>,

    server_frames_moving: SlidingWindowTimely<f32>,
    client_frames_moving: SlidingWindowTimely<f32>,

    history_throughput_weighted: SlidingWindowWeighted<f32>,
    interval_avg_plot_throughput: f32,
    instant_weighted_avg_prev: Instant,

    prev_highest_shard: i32,
    prev_highest_frame: i32,

    stats_history_buffer: VecDeque<HistoryFrame>,
    map_frames_spf: HashMap<u32, usize>,

    is_first_stats: bool,
}

impl StatisticsManager {
    // history size used to calculate average total pipeline latency
    pub fn new(
        max_history_size: usize,
        nominal_server_frame_interval: Duration,
        steamvr_pipeline_frames: f32,
    ) -> Self {
        Self {
            history_buffer: VecDeque::new(),
            max_history_size,

            last_full_report_instant: Instant::now(),
            last_nominal_bitrate_stats: NominalBitrateStats::default(),

            last_frame_present_instant: Instant::now(),
            last_frame_present_interval: Duration::ZERO,

            last_vsync_time: Instant::now(),

            video_packets_total: 0,
            video_packets_partial_sum: 0,

            video_bytes_total: 0,
            video_bytes_partial_sum: 0,

            video_shards_sent_partial_sum: 0,

            video_shards_lost_partial_sum: 0,

            received_video_bytes_partial_sum: 0.,

            frame_interarrival_partial_sum: 0.,

            packets_dropped_total: 0,
            packets_dropped_partial_sum: 0,

            packets_skipped_total: 0,
            packets_skipped_partial_sum: 0,

            battery_gauges: HashMap::new(),
            steamvr_pipeline_latency: Duration::from_secs_f32(
                steamvr_pipeline_frames * nominal_server_frame_interval.as_secs_f32(),
            ),

            total_pipeline_latency_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            game_delay_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            server_compositor_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            encode_delay_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            network_delay_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            decode_delay_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            decoder_queue_delay_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            client_compositor_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),
            vsync_queue_delay_average: SlidingWindowAverage::new(
                Duration::ZERO,
                Some(max_history_size),
                None,
                None,
            ),

            frame_interval: nominal_server_frame_interval,

            frame_interval_average: SlidingWindowAverage::new(
                Duration::from_millis(16),
                Some(max_history_size),
                None,
                None,
            ),
            client_frame_interval_average: SlidingWindowAverage::new(
                Duration::from_millis(16),
                Some(max_history_size),
                None,
                None,
            ),

            frame_interarrival_average: SlidingWindowAverage::new(
                0.,
                Some(max_history_size),
                None,
                None,
            ),

            server_frames_moving: SlidingWindowTimely::new(60., 16., 1.),
            client_frames_moving: SlidingWindowTimely::new(60., 16., 1.),

            history_throughput_weighted: SlidingWindowWeighted::new(0., 0.0),
            instant_weighted_avg_prev: Instant::now(),
            interval_avg_plot_throughput: 0. as f32,

            prev_highest_shard: -1,
            prev_highest_frame: 0,

            stats_history_buffer: VecDeque::new(),
            map_frames_spf: HashMap::new(),

            is_first_stats: true,
        }
    }

    pub fn report_tracking_received(&mut self, target_timestamp: Duration) {
        if !self
            .history_buffer
            .iter()
            .any(|frame| frame.target_timestamp == target_timestamp)
        {
            self.history_buffer.push_front(HistoryFrame {
                target_timestamp,
                tracking_received: Instant::now(),
                ..Default::default()
            });
        }

        if self.history_buffer.len() > self.max_history_size {
            self.history_buffer.pop_back();
        }
    }

    pub fn report_frame_present(&mut self, target_timestamp: Duration, offset: Duration) {
        if let Some(frame) = self
            .history_buffer
            .iter_mut()
            .find(|frame| frame.target_timestamp == target_timestamp)
        {
            let now = Instant::now() - offset;

            let interval = now.saturating_duration_since(self.last_frame_present_instant);

            self.last_frame_present_interval = interval;
            self.last_frame_present_instant = now;

            frame.frame_present = now;

            self.frame_interval_average
                .submit_sample(self.last_frame_present_interval);

            self.server_frames_moving
                .submit_sample(1., interval.as_secs_f32());

            self.stats_history_buffer.push_back(frame.clone());

            if self.stats_history_buffer.len() > self.max_history_size {
                self.stats_history_buffer.pop_front();
            }
        }
    }

    pub fn report_frame_composed(&mut self, target_timestamp: Duration, offset: Duration) {
        if let Some(frame) = self
            .stats_history_buffer
            .iter_mut()
            .find(|frame| frame.target_timestamp == target_timestamp && !frame.is_composed)
        {
            frame.is_composed = true;

            frame.frame_composed = Instant::now() - offset;
        }
    }

    // returns encoding interval
    pub fn report_frame_encoded(
        &mut self,
        target_timestamp: Duration,
        bytes_count: usize,
        is_idr: bool,
    ) -> Duration {
        self.video_packets_total += 1;
        self.video_packets_partial_sum += 1;
        self.video_bytes_total += bytes_count;
        self.video_bytes_partial_sum += bytes_count;

        if let Some(frame) = self
            .stats_history_buffer
            .iter_mut()
            .find(|frame| frame.target_timestamp == target_timestamp && !frame.is_encoded)
        {
            frame.is_idr = is_idr;
            frame.is_encoded = true;

            frame.frame_encoded = Instant::now();

            frame.video_packet_bytes = bytes_count;

            frame
                .frame_encoded
                .saturating_duration_since(frame.frame_composed)
        } else {
            Duration::ZERO
        }
    }

    pub fn report_frame_sent(
        &mut self,
        target_timestamp: Duration,
        frame_index: u32,
        shards_count: usize,
    ) {
        if let Some(frame) = self
            .stats_history_buffer
            .iter_mut()
            .find(|frame| frame.target_timestamp == target_timestamp && frame.frame_index == -1)
        {
            frame.frame_index = frame_index as i32;
        }
        self.map_frames_spf.insert(frame_index, shards_count);
    }

    pub fn report_battery(&mut self, device_id: u64, gauge_value: f32, is_plugged: bool) {
        *self.battery_gauges.entry(device_id).or_default() = BatteryData {
            gauge_value,
            is_plugged,
        };
    }

    pub fn report_nominal_bitrate_stats(&mut self, stats: NominalBitrateStats) {
        self.last_nominal_bitrate_stats = stats;
    }

    // This statistics are reported for every succesfully received frame
    pub fn report_network_statistics(
        &mut self,
        network_stats: NetworkStatisticsPacket,
        rtt: Duration,
    ) -> (f32, f32) {
        self.packets_skipped_total += network_stats.frames_skipped as usize;
        self.packets_skipped_partial_sum += network_stats.frames_skipped as usize;

        self.received_video_bytes_partial_sum += network_stats.rx_bytes as f32;

        self.frame_interarrival_partial_sum += network_stats.frame_interarrival;

        let mut frame_interarrival = network_stats.frame_interarrival;
        if !self.is_first_stats {
            self.frame_interarrival_average
                .submit_sample(frame_interarrival);
        } else {
            frame_interarrival = 0.011;
            self.is_first_stats = false;
        }

        let peak_network_throughput_bps: f32 = if network_stats.frame_span != 0.0 {
            network_stats.bytes_in_frame as f32 * 8.0 / network_stats.frame_span
        } else {
            0.0
        };

        let instant_network_throughput_bps: f32 = if network_stats.frame_interarrival != 0.0 {
            network_stats.rx_bytes as f32 * 8.0 / network_stats.frame_interarrival
        } else {
            0.0
        };

        self.history_throughput_weighted.submit_sample(
            instant_network_throughput_bps,
            network_stats.frame_interarrival,
        );

        let mut shards_sent: usize = 0;
        let shards_lost: isize;

        if self.prev_highest_frame == network_stats.highest_rx_frame_index as i32 {
            if self.prev_highest_shard < network_stats.highest_rx_shard_index as i32 {
                shards_sent =
                    (network_stats.highest_rx_shard_index - self.prev_highest_shard) as usize;

                self.prev_highest_shard = network_stats.highest_rx_shard_index as i32;
            }
        } else if self.prev_highest_frame < network_stats.highest_rx_frame_index as i32 {
            let shards_from_prev = match self.map_frames_spf.get(&(self.prev_highest_frame as u32))
            {
                Some(&shards_count_prev) => {
                    shards_count_prev.saturating_sub((self.prev_highest_shard + 1) as usize)
                }
                None => 0,
            };

            let shards_from_inbetween: usize = self
                .map_frames_spf
                .iter()
                .filter(|&(frame, _)| {
                    *frame > self.prev_highest_frame as u32
                        && *frame < network_stats.highest_rx_frame_index as u32
                })
                .map(|(_, val)| *val)
                .sum();

            let shards_from_actual = network_stats.highest_rx_shard_index as usize + 1;

            shards_sent = shards_from_prev + shards_from_inbetween + shards_from_actual;
        }

        shards_lost = shards_sent as isize - network_stats.rx_shard_counter as isize;

        self.video_shards_sent_partial_sum += shards_sent as isize;
        self.video_shards_lost_partial_sum += shards_lost;

        self.prev_highest_frame = network_stats.highest_rx_frame_index as i32;
        self.prev_highest_shard = network_stats.highest_rx_shard_index as i32;

        let keys_to_drop: Vec<_> = self
            .map_frames_spf
            .iter()
            .filter(|&(frame, _)| *frame < self.prev_highest_frame as u32)
            .map(|(key, _)| *key)
            .collect();

        for key in keys_to_drop {
            self.map_frames_spf.remove_entry(&key);
        }

        if Instant::now().duration_since(self.instant_weighted_avg_prev) >= Duration::from_secs(1) {
            self.instant_weighted_avg_prev = Instant::now();
            self.interval_avg_plot_throughput = self.history_throughput_weighted.get_average();
        }

        alvr_events::send_event(EventType::GraphNetworkStatistics(GraphNetworkStatistics {
            frame_index: network_stats.frame_index as u32,

            server_fps: 1.
                / self
                    .server_frames_moving
                    .get_interval_buffer_mean()
                    .max(Duration::from_millis(1).as_secs_f32()),

            client_fps: 1.
                / self
                    .client_frames_moving
                    .get_interval_buffer_mean()
                    .max(Duration::from_millis(1).as_secs_f32()),

            frame_span_ms: network_stats.frame_span * 1000.0,

            interarrival_jitter_ms: network_stats.interarrival_jitter * 1000.0,

            ow_delay_ms: network_stats.ow_delay * 1000.0,
            filtered_ow_delay_ms: network_stats.filtered_ow_delay * 1000.0,

            rtt_ms: rtt.as_secs_f32() * 1000.0,

            frame_interarrival_ms: network_stats.frame_interarrival * 1000.0,
            frame_jitter_ms: self.frame_interarrival_average.get_std() * 1000.0,

            frames_skipped: network_stats.frames_skipped,

            shards_lost: shards_lost,
            shards_duplicated: network_stats.duplicated_shard_counter,
            shards_sent: shards_sent as u32,

            instant_network_throughput_bps: instant_network_throughput_bps,
            peak_network_throughput_bps: peak_network_throughput_bps,

            nominal_bitrate: self.last_nominal_bitrate_stats.clone(),

            interval_avg_plot_throughput: self.interval_avg_plot_throughput,
        }));

        return (peak_network_throughput_bps, frame_interarrival);
    }

    pub fn report_statistics_summary(&mut self) {
        let now = Instant::now();
        if self.last_full_report_instant + FULL_REPORT_INTERVAL < now {
            let interval_secs = now
                .saturating_duration_since(self.last_full_report_instant)
                .as_secs_f32();
            let shard_loss_rate = if self.video_shards_sent_partial_sum == 0 {
                0.0
            } else {
                self.video_shards_lost_partial_sum as f32
                    / self.video_shards_sent_partial_sum as f32
            };

            alvr_events::send_event(EventType::StatisticsSummary(StatisticsSummary {
                video_packets_total: self.video_packets_total,
                video_packets_per_sec: (self.video_packets_partial_sum as f32 / interval_secs) as _,

                video_mbytes_total: (self.video_bytes_total as f32 / 1e6) as usize,
                video_mbits_per_sec: self.video_bytes_partial_sum as f32 * 8. / 1e6 / interval_secs,

                video_throughput_mbits_per_sec: self.received_video_bytes_partial_sum as f32 * 8.
                    / 1e6
                    / self.frame_interarrival_partial_sum,

                total_pipeline_latency_average_ms: self
                    .total_pipeline_latency_average
                    .get_average()
                    .as_secs_f32()
                    * 1000.,
                game_delay_average_ms: self.game_delay_average.get_average().as_secs_f32() * 1000.,
                server_compositor_delay_average_ms: self
                    .server_compositor_average
                    .get_average()
                    .as_secs_f32()
                    * 1000.,
                encode_delay_average_ms: self.encode_delay_average.get_average().as_secs_f32()
                    * 1000.,
                network_delay_average_ms: self.network_delay_average.get_average().as_secs_f32()
                    * 1000.,
                decode_delay_average_ms: self.decode_delay_average.get_average().as_secs_f32()
                    * 1000.,
                decoder_queue_delay_average_ms: self
                    .decoder_queue_delay_average
                    .get_average()
                    .as_secs_f32()
                    * 1000.,
                client_compositor_average_ms: self
                    .client_compositor_average
                    .get_average()
                    .as_secs_f32()
                    * 1000.,
                vsync_queue_delay_average_ms: self
                    .vsync_queue_delay_average
                    .get_average()
                    .as_secs_f32()
                    * 1000.,

                packets_dropped_total: self.packets_dropped_total,
                packets_dropped_per_sec: (self.packets_dropped_partial_sum as f32 / interval_secs)
                    as _,
                packets_skipped_total: self.packets_skipped_total,
                packets_skipped_per_sec: (self.packets_skipped_partial_sum as f32 / interval_secs)
                    as _,

                shard_loss_rate: shard_loss_rate,

                frame_jitter_ms: self.frame_interarrival_average.get_std() * 1000.0,

                client_fps: 1.0
                    / self
                        .client_frame_interval_average
                        .get_average()
                        .max(Duration::from_millis(1))
                        .as_secs_f32(),
                server_fps: 1.0
                    / self
                        .frame_interval_average
                        .get_average()
                        .max(Duration::from_millis(1))
                        .as_secs_f32(),

                battery_hmd: (self
                    .battery_gauges
                    .get(&HEAD_ID)
                    .cloned()
                    .unwrap_or_default()
                    .gauge_value
                    * 100.) as u32,
                hmd_plugged: self
                    .battery_gauges
                    .get(&HEAD_ID)
                    .cloned()
                    .unwrap_or_default()
                    .is_plugged,
            }));

            self.video_packets_partial_sum = 0;
            self.video_bytes_partial_sum = 0;

            self.received_video_bytes_partial_sum = 0.;

            self.frame_interarrival_partial_sum = 0.;

            self.packets_dropped_partial_sum = 0;
            self.packets_skipped_partial_sum = 0;

            self.video_shards_sent_partial_sum = 0;
            self.video_shards_lost_partial_sum = 0;

            self.last_full_report_instant = now;
        }
    }

    // This statistics are reported for every succesfully displayed frame
    // Returns network latency, frame interarrival average
    pub fn report_statistics(&mut self, client_stats: ClientStatistics) -> Duration {
        if let Some(frame) = self
            .stats_history_buffer
            .iter_mut()
            .find(|frame| frame.frame_index == client_stats.frame_index)
        {
            self.packets_dropped_total += client_stats.frames_dropped as usize;
            self.packets_dropped_partial_sum += client_stats.frames_dropped as usize;

            self.client_frame_interval_average
                .submit_sample(client_stats.frame_interval);

            self.client_frames_moving
                .submit_sample(1., client_stats.frame_interval.as_secs_f32());

            let total_pipeline_latency = client_stats.total_pipeline_latency;

            let game_time_latency = frame
                .frame_present
                .saturating_duration_since(frame.tracking_received);

            let server_compositor_latency = frame
                .frame_composed
                .saturating_duration_since(frame.frame_present);

            let encoder_latency = frame
                .frame_encoded
                .saturating_duration_since(frame.frame_composed);

            // The network latency cannot be estiamed directly. It is what's left of the total
            // latency after subtracting all other latency intervals. In particular it contains the
            // transport latency of the tracking packet and the interval between the first video
            // packet is sent and the last video packet is received for a specific frame.
            // For safety, use saturating_sub to avoid a crash if for some reason the network
            // latency is miscalculated as negative.
            let network_latency = total_pipeline_latency.saturating_sub(
                game_time_latency
                    + server_compositor_latency
                    + encoder_latency
                    + client_stats.video_decode
                    + client_stats.video_decoder_queue
                    + client_stats.rendering
                    + client_stats.vsync_queue,
            );

            self.total_pipeline_latency_average
                .submit_sample(total_pipeline_latency);
            self.game_delay_average.submit_sample(game_time_latency);
            self.server_compositor_average
                .submit_sample(server_compositor_latency);
            self.encode_delay_average.submit_sample(encoder_latency);
            self.network_delay_average.submit_sample(network_latency);
            self.decode_delay_average
                .submit_sample(client_stats.video_decode);
            self.decoder_queue_delay_average
                .submit_sample(client_stats.video_decoder_queue);
            self.client_compositor_average
                .submit_sample(client_stats.rendering);
            self.vsync_queue_delay_average
                .submit_sample(client_stats.vsync_queue);

            /*
            let client_fps = 1.0
                / client_stats
                    .frame_interval
                    .max(Duration::from_millis(1))
                    .as_secs_f32();
            let server_fps = 1.0
                / self
                    .last_frame_present_interval
                    .max(Duration::from_millis(1))
                    .as_secs_f32();
            */

            let bitrate_bps = if network_latency != Duration::ZERO {
                frame.video_packet_bytes as f32 * 8.0 / network_latency.as_secs_f32()
            } else {
                0.0
            };

            // todo: use target timestamp in nanoseconds. the dashboard needs to use the first
            // timestamp as the graph time origin.
            alvr_events::send_event(EventType::GraphStatistics(GraphStatistics {
                frame_index: client_stats.frame_index, // added

                frames_dropped: client_stats.frames_dropped, // added

                total_pipeline_latency_s: client_stats.total_pipeline_latency.as_secs_f32(),
                game_time_s: game_time_latency.as_secs_f32(),
                server_compositor_s: server_compositor_latency.as_secs_f32(),
                encoder_s: encoder_latency.as_secs_f32(),
                network_s: network_latency.as_secs_f32(),
                decoder_s: client_stats.video_decode.as_secs_f32(),
                decoder_queue_s: client_stats.video_decoder_queue.as_secs_f32(),
                client_compositor_s: client_stats.rendering.as_secs_f32(),
                vsync_queue_s: client_stats.vsync_queue.as_secs_f32(),

                // client_fps, // removed
                // server_fps, // removed
                nominal_bitrate: self.last_nominal_bitrate_stats.clone(),
                actual_bitrate_bps: bitrate_bps, // bitrate as computed by ALVR
            }));

            self.report_statistics_summary();

            return network_latency;
        } else {
            Duration::ZERO
        }
    }

    pub fn video_pipeline_latency_average(&self) -> Duration {
        self.total_pipeline_latency_average.get_average()
    }

    pub fn tracker_pose_time_offset(&self) -> Duration {
        // This is the opposite of the client's StatisticsManager::tracker_prediction_offset().
        self.steamvr_pipeline_latency
            .saturating_sub(self.total_pipeline_latency_average.get_average())
    }

    // NB: this call is non-blocking, waiting should be done externally
    pub fn duration_until_next_vsync(&mut self) -> Duration {
        let now = Instant::now();

        // update the last vsync if it's too old
        while self.last_vsync_time + self.frame_interval < now {
            self.last_vsync_time += self.frame_interval;
        }

        (self.last_vsync_time + self.frame_interval).saturating_duration_since(now)
    }
}
