//! 所属模块：plugin / ARA2 宿主 / C API 绑定
//! 文件用途：
//! - 手写 Celemony ARA C API 的 Rust 镜像（`vendor/ARA_SDK/ARA_API/ARAInterface.h`）。
//! - 只覆盖宿主端用到的结构：工厂、文档控制器、宿主控制器、模型对象属性与内容事件。
//!
//! 结构体布局必须与 SDK 完全一致：ARA 在 x86 / x64 上使用 `#pragma pack(push, 1)`，
//! 因此这里全部使用 `#[repr(C, packed(1))]`，访问字段时一律通过值拷贝，不能取引用。
//!
//! Module: plugin / ARA2 host / C API bindings
//! Purpose:
//! - Hand-written Rust mirror of the Celemony ARA C API
//!   (`vendor/ARA_SDK/ARA_API/ARAInterface.h`).
//! - Only the host-side structures the host uses: factory, document controller, host
//!   controllers, model object properties and content events.
//!
//! The layout must match the SDK exactly: ARA uses `#pragma pack(push, 1)` on x86 / x64, so
//! every struct here is `#[repr(C, packed(1))]` and fields are only ever copied by value.

#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::ffi::{c_char, c_void};

/// `ARASize`
pub type AraSize = usize;
/// `ARABool`
pub type AraBool = i32;
/// `ARAInt32`
pub type AraInt32 = i32;
/// `ARASampleCount`
pub type AraSampleCount = i64;
/// `ARASampleRate`
pub type AraSampleRate = f64;
/// `ARAChannelCount`
pub type AraChannelCount = i32;
/// `ARATimePosition` / `ARATimeDuration` / `ARAQuarterPosition`
pub type AraTimePosition = f64;
/// `ARAUtf8String`
pub type AraUtf8String = *const c_char;
/// `ARAPersistentID`
pub type AraPersistentId = *const c_char;
/// 插件侧不透明引用（`ARA_REF`）。
/// Opaque plug-in side reference (`ARA_REF`).
pub type AraRef = *mut c_void;
/// 宿主侧不透明引用（`ARA_HOST_REF`）。
/// Opaque host side reference (`ARA_HOST_REF`).
pub type AraHostRef = *mut c_void;

/// `kARATrue`
pub const ARA_TRUE: AraBool = 1;
/// `kARAFalse`
pub const ARA_FALSE: AraBool = 0;

/// `kARAAPIGeneration_2_0_Final`
pub const ARA_API_GENERATION_2_0_FINAL: i32 = 4;

/// `kARAContentTypeTempoEntries`
pub const ARA_CONTENT_TYPE_TEMPO_ENTRIES: i32 = 20;
/// `kARAContentTypeBarSignatures`
pub const ARA_CONTENT_TYPE_BAR_SIGNATURES: i32 = 21;

/// 插件实例角色：播放渲染 + 编辑器渲染 + 编辑器视图。
/// Plug-in instance roles: playback render + editor render + editor view.
pub const ARA_ROLE_ALL: i32 = 0b111;

/// `ARAColor`
#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
pub struct AraColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// `ARAContentTimeRange`
#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
pub struct AraContentTimeRange {
    pub start: AraTimePosition,
    pub duration: AraTimePosition,
}

/// `ARAContentTempoEntry`
#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
pub struct AraContentTempoEntry {
    pub time_position: AraTimePosition,
    pub quarter_position: f64,
}

/// `ARAContentBarSignature`
#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
pub struct AraContentBarSignature {
    pub numerator: AraInt32,
    pub denominator: AraInt32,
    pub position: f64,
}

/// `ARADocumentProperties`
#[repr(C, packed(1))]
pub struct AraDocumentProperties {
    pub struct_size: AraSize,
    pub name: AraUtf8String,
}

/// `ARAMusicalContextProperties`
#[repr(C, packed(1))]
pub struct AraMusicalContextProperties {
    pub struct_size: AraSize,
    pub name: AraUtf8String,
    pub order_index: AraInt32,
    pub color: *const AraColor,
}

/// `ARARegionSequenceProperties`
#[repr(C, packed(1))]
pub struct AraRegionSequenceProperties {
    pub struct_size: AraSize,
    pub name: AraUtf8String,
    pub order_index: AraInt32,
    pub musical_context_ref: AraRef,
    pub color: *const AraColor,
}

/// `ARAAudioSourceProperties`
#[repr(C, packed(1))]
pub struct AraAudioSourceProperties {
    pub struct_size: AraSize,
    pub name: AraUtf8String,
    pub persistent_id: AraPersistentId,
    pub sample_count: AraSampleCount,
    pub sample_rate: AraSampleRate,
    pub channel_count: AraChannelCount,
    pub merits64_bit_samples: AraBool,
    pub channel_arrangement_data_type: AraInt32,
    pub channel_arrangement: *const c_void,
}

/// `ARAAudioModificationProperties`
#[repr(C, packed(1))]
pub struct AraAudioModificationProperties {
    pub struct_size: AraSize,
    pub name: AraUtf8String,
    pub persistent_id: AraPersistentId,
}

/// `ARAPlaybackRegionProperties`
#[repr(C, packed(1))]
pub struct AraPlaybackRegionProperties {
    pub struct_size: AraSize,
    pub transformation_flags: AraInt32,
    pub start_in_modification_time: AraTimePosition,
    pub duration_in_modification_time: AraTimePosition,
    pub start_in_playback_time: AraTimePosition,
    pub duration_in_playback_time: AraTimePosition,
    pub musical_context_ref: AraRef,
    pub region_sequence_ref: AraRef,
    pub name: AraUtf8String,
    pub color: *const AraColor,
}

/// `ARAInterfaceConfiguration`
#[repr(C, packed(1))]
pub struct AraInterfaceConfiguration {
    pub struct_size: AraSize,
    pub desired_api_generation: i32,
    /// 指向函数指针的指针；可为 NULL 指针本身，或指向空函数指针以关闭断言。
    /// Pointer to a function pointer; may be null, or point to a null function pointer.
    pub assert_function_address: *mut c_void,
}

/// `ARAAssertCategory`
pub type AraAssertCategory = i32;
/// `ARAAssertFunction`
pub type AraAssertFunction =
    Option<unsafe extern "C" fn(AraAssertCategory, *const c_void, *const c_char)>;

/// `ARAAudioAccessControllerInterface`
#[repr(C, packed(1))]
pub struct AraAudioAccessControllerInterface {
    pub struct_size: AraSize,
    pub create_audio_reader_for_source:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraBool) -> AraHostRef>,
    pub read_audio_samples: Option<
        unsafe extern "C" fn(
            AraHostRef,
            AraHostRef,
            AraSampleCount,
            AraSampleCount,
            *const *mut c_void,
        ) -> AraBool,
    >,
    pub destroy_audio_reader: Option<unsafe extern "C" fn(AraHostRef, AraHostRef)>,
}

/// `ARAArchivingControllerInterface`
#[repr(C, packed(1))]
pub struct AraArchivingControllerInterface {
    pub struct_size: AraSize,
    pub get_archive_size: Option<unsafe extern "C" fn(AraHostRef, AraHostRef) -> AraSize>,
    pub read_bytes_from_archive:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraSize, AraSize, *mut u8) -> AraBool>,
    pub write_bytes_to_archive: Option<
        unsafe extern "C" fn(AraHostRef, AraHostRef, AraSize, AraSize, *const u8) -> AraBool,
    >,
    pub notify_document_archiving_progress: Option<unsafe extern "C" fn(AraHostRef, f32)>,
    pub notify_document_unarchiving_progress: Option<unsafe extern "C" fn(AraHostRef, f32)>,
    pub get_document_archive_id:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef) -> AraPersistentId>,
}

/// `ARAContentAccessControllerInterface`
#[repr(C, packed(1))]
pub struct AraContentAccessControllerInterface {
    pub struct_size: AraSize,
    pub is_musical_context_content_available:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraInt32) -> AraBool>,
    pub get_musical_context_content_grade:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraInt32) -> AraInt32>,
    pub create_musical_context_content_reader: Option<
        unsafe extern "C" fn(
            AraHostRef,
            AraHostRef,
            AraInt32,
            *const AraContentTimeRange,
        ) -> AraHostRef,
    >,
    pub is_audio_source_content_available:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraInt32) -> AraBool>,
    pub get_audio_source_content_grade:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraInt32) -> AraInt32>,
    pub create_audio_source_content_reader: Option<
        unsafe extern "C" fn(
            AraHostRef,
            AraHostRef,
            AraInt32,
            *const AraContentTimeRange,
        ) -> AraHostRef,
    >,
    pub get_content_reader_event_count:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef) -> AraInt32>,
    pub get_content_reader_data_for_event:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraInt32) -> *const c_void>,
    pub destroy_content_reader: Option<unsafe extern "C" fn(AraHostRef, AraHostRef)>,
}

/// `ARAModelUpdateControllerInterface`
#[repr(C, packed(1))]
pub struct AraModelUpdateControllerInterface {
    pub struct_size: AraSize,
    pub notify_audio_source_analysis_progress:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, AraInt32, f32)>,
    pub notify_audio_source_content_changed:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, *const AraContentTimeRange, AraInt32)>,
    pub notify_audio_modification_content_changed:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, *const AraContentTimeRange, AraInt32)>,
    pub notify_playback_region_content_changed:
        Option<unsafe extern "C" fn(AraHostRef, AraHostRef, *const AraContentTimeRange, AraInt32)>,
    pub notify_document_data_changed: Option<unsafe extern "C" fn(AraHostRef)>,
}

/// `ARAPlaybackControllerInterface`
#[repr(C, packed(1))]
pub struct AraPlaybackControllerInterface {
    pub struct_size: AraSize,
    pub request_start_playback: Option<unsafe extern "C" fn(AraHostRef)>,
    pub request_stop_playback: Option<unsafe extern "C" fn(AraHostRef)>,
    pub request_set_playback_position: Option<unsafe extern "C" fn(AraHostRef, AraTimePosition)>,
    pub request_set_cycle_range:
        Option<unsafe extern "C" fn(AraHostRef, AraTimePosition, AraTimePosition)>,
    pub request_enable_cycle: Option<unsafe extern "C" fn(AraHostRef, AraBool)>,
}

/// `ARADocumentControllerHostInstance`
#[repr(C, packed(1))]
pub struct AraDocumentControllerHostInstance {
    pub struct_size: AraSize,
    pub audio_access_controller_host_ref: AraHostRef,
    pub audio_access_controller_interface: *const AraAudioAccessControllerInterface,
    pub archiving_controller_host_ref: AraHostRef,
    pub archiving_controller_interface: *const AraArchivingControllerInterface,
    pub content_access_controller_host_ref: AraHostRef,
    pub content_access_controller_interface: *const AraContentAccessControllerInterface,
    pub model_update_controller_host_ref: AraHostRef,
    pub model_update_controller_interface: *const AraModelUpdateControllerInterface,
    pub playback_controller_host_ref: AraHostRef,
    pub playback_controller_interface: *const AraPlaybackControllerInterface,
}

/// `ARADocumentControllerInterface`（插件提供，宿主调用；顺序必须与头文件一致）。
/// `ARADocumentControllerInterface` (provided by the plug-in, called by the host; order must
/// match the header).
#[repr(C, packed(1))]
pub struct AraDocumentControllerInterface {
    pub struct_size: AraSize,
    pub destroy_document_controller: Option<unsafe extern "C" fn(AraRef)>,
    pub get_factory: Option<unsafe extern "C" fn(AraRef) -> *const AraFactory>,
    pub begin_editing: Option<unsafe extern "C" fn(AraRef)>,
    pub end_editing: Option<unsafe extern "C" fn(AraRef)>,
    pub notify_model_updates: Option<unsafe extern "C" fn(AraRef)>,
    pub begin_restoring_document_from_archive:
        Option<unsafe extern "C" fn(AraRef, AraHostRef) -> AraBool>,
    pub end_restoring_document_from_archive:
        Option<unsafe extern "C" fn(AraRef, AraHostRef) -> AraBool>,
    pub store_document_to_archive: Option<unsafe extern "C" fn(AraRef, AraHostRef) -> AraBool>,
    pub update_document_properties:
        Option<unsafe extern "C" fn(AraRef, *const AraDocumentProperties)>,
    pub create_musical_context: Option<
        unsafe extern "C" fn(AraRef, AraHostRef, *const AraMusicalContextProperties) -> AraRef,
    >,
    pub update_musical_context_properties:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraMusicalContextProperties)>,
    pub update_musical_context_content:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraContentTimeRange, AraInt32)>,
    pub destroy_musical_context: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub create_audio_source:
        Option<unsafe extern "C" fn(AraRef, AraHostRef, *const AraAudioSourceProperties) -> AraRef>,
    pub update_audio_source_properties:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraAudioSourceProperties)>,
    pub update_audio_source_content:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraContentTimeRange, AraInt32)>,
    pub enable_audio_source_samples_access: Option<unsafe extern "C" fn(AraRef, AraRef, AraBool)>,
    pub deactivate_audio_source_for_undo_history:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraBool)>,
    pub destroy_audio_source: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub create_audio_modification: Option<
        unsafe extern "C" fn(
            AraRef,
            AraRef,
            AraHostRef,
            *const AraAudioModificationProperties,
        ) -> AraRef,
    >,
    pub clone_audio_modification: Option<
        unsafe extern "C" fn(
            AraRef,
            AraRef,
            AraHostRef,
            *const AraAudioModificationProperties,
        ) -> AraRef,
    >,
    pub update_audio_modification_properties:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraAudioModificationProperties)>,
    pub deactivate_audio_modification_for_undo_history:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraBool)>,
    pub destroy_audio_modification: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub create_playback_region: Option<
        unsafe extern "C" fn(
            AraRef,
            AraRef,
            AraHostRef,
            *const AraPlaybackRegionProperties,
        ) -> AraRef,
    >,
    pub update_playback_region_properties:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraPlaybackRegionProperties)>,
    pub destroy_playback_region: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub is_audio_source_content_available:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraBool>,
    pub is_audio_source_content_analysis_incomplete:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraBool>,
    pub request_audio_source_content_analysis:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraSize, *const AraInt32)>,
    pub get_audio_source_content_grade:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraInt32>,
    pub create_audio_source_content_reader: Option<
        unsafe extern "C" fn(AraRef, AraRef, AraInt32, *const AraContentTimeRange) -> AraRef,
    >,
    pub is_audio_modification_content_available:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraBool>,
    pub get_audio_modification_content_grade:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraInt32>,
    pub create_audio_modification_content_reader: Option<
        unsafe extern "C" fn(AraRef, AraRef, AraInt32, *const AraContentTimeRange) -> AraRef,
    >,
    pub is_playback_region_content_available:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraBool>,
    pub get_playback_region_content_grade:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> AraInt32>,
    pub create_playback_region_content_reader: Option<
        unsafe extern "C" fn(AraRef, AraRef, AraInt32, *const AraContentTimeRange) -> AraRef,
    >,
    pub get_content_reader_event_count: Option<unsafe extern "C" fn(AraRef, AraRef) -> AraInt32>,
    pub get_content_reader_data_for_event:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32) -> *const c_void>,
    pub destroy_content_reader: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub create_region_sequence: Option<
        unsafe extern "C" fn(AraRef, AraHostRef, *const AraRegionSequenceProperties) -> AraRef,
    >,
    pub update_region_sequence_properties:
        Option<unsafe extern "C" fn(AraRef, AraRef, *const AraRegionSequenceProperties)>,
    pub destroy_region_sequence: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub get_playback_region_head_and_tail_time:
        Option<unsafe extern "C" fn(AraRef, AraRef, *mut f64, *mut f64)>,
    pub restore_objects_from_archive:
        Option<unsafe extern "C" fn(AraRef, AraHostRef, *const c_void) -> AraBool>,
    pub store_objects_to_archive:
        Option<unsafe extern "C" fn(AraRef, AraHostRef, *const c_void) -> AraBool>,
    pub get_processing_algorithms_count: Option<unsafe extern "C" fn(AraRef) -> AraInt32>,
    pub get_processing_algorithm_properties:
        Option<unsafe extern "C" fn(AraRef, AraInt32) -> *const c_void>,
    pub get_processing_algorithm_for_audio_source:
        Option<unsafe extern "C" fn(AraRef, AraRef) -> AraInt32>,
    pub request_processing_algorithm_for_audio_source:
        Option<unsafe extern "C" fn(AraRef, AraRef, AraInt32)>,
    pub is_licensed_for_capabilities: Option<
        unsafe extern "C" fn(AraRef, AraBool, AraSize, *const AraInt32, AraInt32) -> AraBool,
    >,
    pub store_audio_source_to_audio_file_chunk: Option<
        unsafe extern "C" fn(
            AraRef,
            AraHostRef,
            AraRef,
            *mut AraPersistentId,
            *mut AraBool,
        ) -> AraBool,
    >,
    pub is_audio_modification_preserving_audio_source_signal:
        Option<unsafe extern "C" fn(AraRef, AraRef) -> AraBool>,
}

/// `ARADocumentControllerInstance`
#[repr(C, packed(1))]
pub struct AraDocumentControllerInstance {
    pub struct_size: AraSize,
    pub document_controller_ref: AraRef,
    pub document_controller_interface: *const AraDocumentControllerInterface,
}

/// `ARAFactory`
#[repr(C, packed(1))]
pub struct AraFactory {
    pub struct_size: AraSize,
    pub lowest_supported_api_generation: i32,
    pub highest_supported_api_generation: i32,
    pub factory_id: AraPersistentId,
    pub initialize_ara_with_configuration:
        Option<unsafe extern "C" fn(*const AraInterfaceConfiguration)>,
    pub uninitialize_ara: Option<unsafe extern "C" fn()>,
    pub plug_in_name: AraUtf8String,
    pub manufacturer_name: AraUtf8String,
    pub information_url: AraUtf8String,
    pub version: AraUtf8String,
    pub create_document_controller_with_document: Option<
        unsafe extern "C" fn(
            *const AraDocumentControllerHostInstance,
            *const AraDocumentProperties,
        ) -> *const AraDocumentControllerInstance,
    >,
    pub document_archive_id: AraPersistentId,
    pub compatible_document_archive_ids_count: AraSize,
    pub compatible_document_archive_ids: *const AraPersistentId,
    pub analyzeable_content_types_count: AraSize,
    pub analyzeable_content_types: *const i32,
    pub supported_playback_transformation_flags: i32,
    pub supports_storing_audio_file_chunks: AraBool,
}

/// `ARAPlaybackRendererInterface`
#[repr(C, packed(1))]
pub struct AraPlaybackRendererInterface {
    pub struct_size: AraSize,
    pub add_playback_region: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub remove_playback_region: Option<unsafe extern "C" fn(AraRef, AraRef)>,
}

/// `ARAEditorRendererInterface`
#[repr(C, packed(1))]
pub struct AraEditorRendererInterface {
    pub struct_size: AraSize,
    pub add_playback_region: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub remove_playback_region: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub add_region_sequence: Option<unsafe extern "C" fn(AraRef, AraRef)>,
    pub remove_region_sequence: Option<unsafe extern "C" fn(AraRef, AraRef)>,
}

/// `ARAPlugInExtensionInstance`（ARA 2 角色版；旧字段同时保留）。
/// `ARAPlugInExtensionInstance` (ARA 2 roles; the deprecated fields are kept too).
#[repr(C, packed(1))]
pub struct AraPlugInExtensionInstance {
    pub struct_size: AraSize,
    pub plug_in_extension_ref: AraRef,
    pub plug_in_extension_interface: *const c_void,
    pub playback_renderer_ref: AraRef,
    pub playback_renderer_interface: *const AraPlaybackRendererInterface,
    pub editor_renderer_ref: AraRef,
    pub editor_renderer_interface: *const AraEditorRendererInterface,
    pub editor_view_ref: AraRef,
    pub editor_view_interface: *const c_void,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_the_packed_c_layout() {
        // 每个结构体在 x64 + pack(1) 下都是“字段宽度之和”。
        // Under x64 + pack(1) every struct is the sum of its field widths.
        assert_eq!(std::mem::size_of::<AraColor>(), 12);
        assert_eq!(std::mem::size_of::<AraContentTimeRange>(), 16);
        assert_eq!(std::mem::size_of::<AraContentTempoEntry>(), 16);
        assert_eq!(std::mem::size_of::<AraContentBarSignature>(), 16);
        assert_eq!(std::mem::size_of::<AraDocumentProperties>(), 16);
        assert_eq!(std::mem::size_of::<AraMusicalContextProperties>(), 28);
        assert_eq!(std::mem::size_of::<AraRegionSequenceProperties>(), 36);
        assert_eq!(std::mem::size_of::<AraAudioSourceProperties>(), 60);
        assert_eq!(std::mem::size_of::<AraAudioModificationProperties>(), 24);
        assert_eq!(std::mem::size_of::<AraPlaybackRegionProperties>(), 76);
        assert_eq!(std::mem::size_of::<AraInterfaceConfiguration>(), 20);
        assert_eq!(std::mem::size_of::<AraAudioAccessControllerInterface>(), 32);
        assert_eq!(std::mem::size_of::<AraArchivingControllerInterface>(), 56);
        assert_eq!(
            std::mem::size_of::<AraContentAccessControllerInterface>(),
            80
        );
        assert_eq!(std::mem::size_of::<AraModelUpdateControllerInterface>(), 48);
        assert_eq!(std::mem::size_of::<AraPlaybackControllerInterface>(), 48);
        assert_eq!(std::mem::size_of::<AraDocumentControllerHostInstance>(), 88);
        assert_eq!(std::mem::size_of::<AraDocumentControllerInterface>(), 440);
        assert_eq!(std::mem::size_of::<AraDocumentControllerInstance>(), 24);
        assert_eq!(std::mem::size_of::<AraFactory>(), 128);
        assert_eq!(std::mem::size_of::<AraPlaybackRendererInterface>(), 24);
        assert_eq!(std::mem::size_of::<AraEditorRendererInterface>(), 40);
        assert_eq!(std::mem::size_of::<AraPlugInExtensionInstance>(), 72);
    }
}
