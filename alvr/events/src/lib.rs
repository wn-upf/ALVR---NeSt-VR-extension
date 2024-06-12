use alvr_common::{info, DeviceMotion, LogEntry, Pose, StatesWebrtc};
use alvr_packets::{AudioDevicesList, ButtonValue};
use alvr_session::SessionConfig;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StatisticsSummary {
    pub video_packets_total: usize,
    pub video_packets_per_sec: usize,

    pub video_mbytes_total: usize,
    pub video_mbits_per_sec: f32,

    pub video_throughput_mbits_per_sec: f32,

    pub total_pipeline_latency_average_ms: f32,
    pub game_delay_average_ms: f32,
    pub server_compositor_delay_average_ms: f32,
    pub encode_delay_average_ms: f32,
    pub network_delay_average_ms: f32,
    pub decode_delay_average_ms: f32,
    pub decoder_queue_delay_average_ms: f32,
    pub client_compositor_average_ms: f32,
    pub vsync_queue_delay_average_ms: f32,

    pub packets_dropped_total: usize,
    pub packets_dropped_per_sec: usize,

    pub packets_skipped_total: usize,
    pub packets_skipped_per_sec: usize,

    pub frame_jitter_ms: f32,

    pub client_fps: f32,
    pub server_fps: f32,

    pub battery_hmd: u32,
    pub hmd_plugged: bool,
}

// Bitrate statistics minus the empirical output value
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct NominalBitrateStats {
    pub scaled_calculated_bps: Option<f32>,
    pub decoder_latency_limiter_bps: Option<f32>,
    pub network_latency_limiter_bps: Option<f32>,
    pub encoder_latency_limiter_bps: Option<f32>,
    pub manual_max_bps: Option<f32>,
    pub manual_min_bps: Option<f32>,
    pub requested_bps: f32,
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GraphStatistics {
    pub frame_index: i32,
    pub is_idr: bool,

    pub frames_dropped: u32,

    pub total_pipeline_latency_s: f32,
    pub game_time_s: f32,
    pub server_compositor_s: f32,
    pub encoder_s: f32,
    pub network_s: f32,
    pub decoder_s: f32,
    pub decoder_queue_s: f32,
    pub client_compositor_s: f32,
    pub vsync_queue_s: f32,

    //pub client_fps: f32,
    //pub server_fps: f32,

    pub nominal_bitrate: NominalBitrateStats,
    pub actual_bitrate_bps: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GraphNetworkStatistics {
    pub frame_index: u32,

    pub client_fps: f32,
    pub server_fps: f32,

    pub frame_span_ms: f32,

    pub interarrival_jitter_ms: f32,
    
    pub ow_delay_ms: f32,
    pub filtered_ow_delay_ms: f32,

    pub rtt_alt_ms: f32,


    pub frame_interarrival_ms: f32,
    pub frame_jitter_ms: f32,

    pub frames_skipped: u32,

    pub shards_lost: isize,
    pub shards_duplicated: u32,

    pub network_throughput_bps: f32,

    pub peak_network_throughput_bps: f32,
    pub ema_peak_throughput_bps: f32,

    pub nominal_bitrate: NominalBitrateStats,

    pub threshold_gcc: f32,
    pub internal_state_gcc: StatesWebrtc,
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy, Default)]
pub struct HeuristicStats {
    pub frame_interval: Duration,
    pub framerate: f32,
    pub steps_bps: f32,

    pub fps_heur: f32,
    pub rtt_avg_heur: f32,
    pub random_prob: f32,

    pub threshold_fps: f32,
    pub threshold_rtt: f32,
    pub threshold_u: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrackingEvent {
    pub head_motion: Option<DeviceMotion>,
    pub controller_motions: [Option<DeviceMotion>; 2],
    pub hand_skeletons: [Option<[Pose; 26]>; 2],
    pub eye_gazes: [Option<Pose>; 2],
    pub fb_face_expression: Option<Vec<f32>>,
    pub htc_eye_expression: Option<Vec<f32>>,
    pub htc_lip_expression: Option<Vec<f32>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ButtonEvent {
    pub path: String,
    pub value: ButtonValue,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HapticsEvent {
    pub path: String,
    pub duration: Duration,
    pub frequency: f32,
    pub amplitude: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "id", content = "data")]
pub enum EventType {
    Log(LogEntry),
    Session(Box<SessionConfig>),
    StatisticsSummary(StatisticsSummary),
    GraphStatistics(GraphStatistics),
    GraphNetworkStatistics(GraphNetworkStatistics),
    HeuristicStats(HeuristicStats),
    Tracking(Box<TrackingEvent>),
    Buttons(Vec<ButtonEvent>),
    Haptics(HapticsEvent),
    AudioDevices(AudioDevicesList),
    DriversList(Vec<PathBuf>),
    ServerRequestsSelfRestart,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub timestamp: String,
    pub event_type: EventType,
}

pub fn send_event(event_type: EventType) {
    info!("{}", serde_json::to_string(&event_type).unwrap());
}
