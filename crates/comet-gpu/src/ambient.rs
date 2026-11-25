//! Optional indirect diffuse substitute, not ray-traced GI.
//! Ported from Belobug/VIctory-Belobug-AMD 4cbe9d7 crates/victory-gpu/src/ambient.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug).
//! The upstream resolve writes packed RGB9E5 through a uint texture UAV.
//! Replace that value, rather than accumulate invalid history.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_UINT;

use crate::host::HostList;

pub(crate) const PROGRAM: &[u8] = b"CS_ScreenProbes_TemporalResolve";
pub(crate) const SLOT: u8 = 1;
pub(crate) const DWORD: u32 = 176;
/// The default level, as VIctory-Belobug-AMD's `VICTORY_AMBIENT`.
const DEFAULT_LEVEL: f32 = 0.6;

// Require explicit positive numeric opt-in. Never override output in a
// builder-on experiment (even if the builder subsequently fails).
fn parse_level(value: &str) -> Option<f32> {
    let value: f32 = value.trim().parse().ok()?;
    (value.is_finite() && value > 0.0 && value <= 4.0).then_some(value)
}

pub(crate) fn level() -> Option<f32> {
    static LEVEL: OnceLock<Option<f32>> = OnceLock::new();
    *LEVEL.get_or_init(|| {
        if std::env::var_os("COMET_BVH_BUILDER").is_some_and(|v| v == "1") {
            return None;
        }
        // On by default at VIctory's level (0.6), measured on the reference
        // machine: faces, bunks and the cell lit instead of black.
        // `COMET_AMBIENT=0` (or `off`) turns it off.
        match std::env::var("COMET_AMBIENT") {
            Err(_) => Some(DEFAULT_LEVEL),
            Ok(v) => parse_level(&v),
        }
    })
}

// Neutral RGB, using the RGB9E5 shared exponent. CPU packing lets us issue
// a typed integer UAV clear without another shader, root signature or PSO.
fn pack_neutral(value: f32) -> u32 {
    let mut exponent = ((value.max(2.0f32.powi(-24)).log2().floor() as i32) + 16)
        .clamp(0, 31);
    let mut mantissa = (value * 2.0f32.powi(24 - exponent)).round() as u32;
    if mantissa == 512 && exponent < 31 {
        exponent += 1;
        mantissa = (value * 2.0f32.powi(24 - exponent)).round() as u32;
    }
    let m = mantissa.min(511);
    m | (m << 9) | (m << 18) | ((exponent as u32) << 27)
}

pub(crate) struct Target {
    pub(crate) texture: ID3D12Resource,
    pub(crate) title: usize,
    pub(crate) hash: String,
}

// Check the title's exact descriptor as well as the actual host resource.
// Do not reinterpret typeless resources, depth twins, placed UAVs, slices
// or nonzero mips. A default view is mip zero of this 2D uint resource.
pub(crate) fn compatible(texture: &ID3D12Resource, words: Option<[u32; 10]>) -> bool {
    // SAFETY: a live retained resource.
    let d = unsafe { texture.GetDesc() };
    d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D
        && d.Format == DXGI_FORMAT_R32_UINT
        && d.DepthOrArraySize == 1
        && d.SampleDesc.Count == 1
        && d.Flags.0 & D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS.0 != 0
        && d.Width > 0
        && d.Height > 0
        && words.is_none_or(|w| {
            w[0] == DXGI_FORMAT_R32_UINT.0 as u32
                && w[1] == D3D12_UAV_DIMENSION_TEXTURE2D.0 as u32
                && w[2] == 0
                && w[3] == 0
        })
}

// Separate immutable one-entry heaps per clear; the list holds both until
// retirement. No ring slot can be overwritten by another recording list.
fn heap(device: &ID3D12Device, flags: D3D12_DESCRIPTOR_HEAP_FLAGS)
    -> windows::core::Result<ID3D12DescriptorHeap>
{
    // SAFETY: valid one-entry CBV/SRV/UAV heap description.
    unsafe {
        device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: 1,
            Flags: flags,
            NodeMask: 0,
        })
    }
}

fn barrier(list: &HostList) {
    // Order the title's UAV writes, the clear, and subsequent consumers.
    // A global UAV barrier avoids borrowing a COM pointer in the union.
    // SAFETY: open list, valid global UAV barrier.
    unsafe {
        list.list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                    pResource: std::mem::ManuallyDrop::new(None),
                }),
            },
        }]);
    }
}

// True only if a clear was recorded; caller restores title descriptor heaps
// and invalidates its cached root signatures. No pipeline state is changed.
pub(crate) fn record(list: &HostList, target: &Target, level: f32) -> bool {
    let Some(host) = crate::host::host() else { return false };
    let (Ok(cpu_heap), Ok(gpu_heap)) = (
        heap(&host.device, D3D12_DESCRIPTOR_HEAP_FLAG_NONE),
        heap(&host.device, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE),
    ) else {
        tracing::warn!("B1 ambient: descriptor heap creation failed");
        return false;
    };
    let view = D3D12_UNORDERED_ACCESS_VIEW_DESC {
        Format: DXGI_FORMAT_R32_UINT,
        ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
        ..Default::default()
    };
    let packed = pack_neutral(level);
    if packed == 0 {
        return false;
    }
    list.keep(&target.texture);
    list.keep(&cpu_heap);
    list.keep(&gpu_heap);
    crate::list::ensure_state(list, target.title as crate::object::This, 0x8);
    barrier(list);
    // SAFETY: both descriptors are identical mip-zero views of a retained
    // uint UAV; the GPU descriptor's heap is bound and the CPU descriptor
    // is not shader visible. The resource is in unordered-access state.
    unsafe {
        let cpu = cpu_heap.GetCPUDescriptorHandleForHeapStart();
        host.device.CreateUnorderedAccessView(
            &target.texture, None::<&ID3D12Resource>, Some(&view), cpu,
        );
        host.device.CreateUnorderedAccessView(
            &target.texture, None::<&ID3D12Resource>, Some(&view),
            gpu_heap.GetCPUDescriptorHandleForHeapStart(),
        );
        list.list.SetDescriptorHeaps(&[Some(gpu_heap.clone())]);
        list.list.ClearUnorderedAccessViewUint(
            gpu_heap.GetGPUDescriptorHandleForHeapStart(),
            cpu,
            &target.texture,
            &[packed, 0, 0, 0],
            &[],
        );
    }
    barrier(list);
    static RECORDED: AtomicU64 = AtomicU64::new(0);
    let n = RECORDED.fetch_add(1, Ordering::Relaxed) + 1;
    if n <= 16 || n.is_power_of_two() {
        tracing::info!(hash = %target.hash, frame = crate::frame::current(), level,
            packed, title = target.title, n, "B1 ambient UAV clear recorded");
    }
    #[cfg(feature = "diagnostics")]
    list.note_op(|| format!("B1 ambient UAV clear recorded {} title {:#x} packed {packed:#x}",
        target.hash, target.title));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_level_is_explicit_and_bounded() {
        for invalid in ["", "off", "0", "-1", "NaN", "inf", "4.1"] {
            assert_eq!(parse_level(invalid), None);
        }
        assert_eq!(parse_level("0.6"), Some(0.6));
        assert_eq!(parse_level("4"), Some(4.0));
    }

    #[test]
    fn ambient_packs_rgb9e5() {
        assert_eq!(pack_neutral(0.5), 0x7c020100);
        assert_eq!(pack_neutral(1.0), 0x84020100);
        assert_eq!(pack_neutral(0.0), 0);
        let bits = pack_neutral(0.6);
        let m = bits & 511;
        assert_eq!(m, (bits >> 9) & 511);
        assert_eq!(m, (bits >> 18) & 511);
        let decoded = m as f32 * 2.0f32.powi((bits >> 27) as i32 - 24);
        assert!((decoded - 0.6).abs() < 0.002);
    }
}
