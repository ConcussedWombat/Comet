//! Sky-coloured stand-in only: not title probe tracing or bounced light.
//! Adapted from Belobug/VIctory-Belobug-AMD e48eb83, 4f62c2a
//! (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug).

use super::{HostList, Target, barrier};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R32_UINT};

pub(crate) const PROGRAM: &[u8] = b"ps_sky_paraboloid_reflection";
const DEFAULT: [f32; 3] = [0.36, 0.05, 0.4];

fn parse(value: &str) -> Option<[f32; 3]> {
    if matches!(value.trim(), "1" | "on") {
        return Some(DEFAULT);
    }
    let mut f = value.split(',');
    let gain: f32 = f.next()?.trim().parse().ok()?;
    let least: f32 = f.next()?.trim().parse().ok()?;
    let power: f32 = f.next()?.trim().parse().ok()?;
    (f.next().is_none() && (0.001..=4.0).contains(&gain)
        && (0.0..=4.0).contains(&least) && (0.05..=2.0).contains(&power))
        .then_some([gain, least, power])
}

fn settings() -> Option<[f32; 3]> {
    static SET: OnceLock<Option<[f32; 3]>> = OnceLock::new();
    *SET.get_or_init(|| std::env::var("COMET_AMBIENT_SKY").ok().as_deref().and_then(parse))
}

// B1 owns depth shading and tint. Until the combined pass is reviewed,
// don't silently override either explicit setting.
pub(crate) fn enabled() -> bool {
    settings().is_some() && super::level().is_some()
        && ["COMET_AMBIENT_SHADE", "COMET_AMBIENT_TINT"].iter().all(|key|
            std::env::var(key).ok().is_none_or(|v| matches!(v.trim(), "" | "0" | "off")))
}

struct Dome {
    copy: Arc<crate::resource::HostTexture>,
    hash: String,
    frame: u64,
}
static DOME: Mutex<Option<Dome>> = Mutex::new(None);

// Called only AFTER a nonempty direct/indexed draw has been recorded.
// Publish from submission preparation, not parallel list recording.
// On the ordinary single direct queue a subsequent submission observes
// the producer before the reader; the first use may be the previous dome.
// Hold the exact host copy rather than a potentially recycled title pointer.
pub(crate) fn drawn(list: &HostList, title: crate::object::This, hash: &str) {
    if title.is_null() { return; }
    let Some(copy) = crate::resource::host_copy(title) else { return };
    // SAFETY: retained live host resource.
    let d = unsafe { copy.texture.GetDesc() };
    if d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
        || d.Format != DXGI_FORMAT_R16G16B16A16_FLOAT
        || d.Width != 128 || d.Height != 128 || d.DepthOrArraySize != 1
        || d.SampleDesc.Count != 1 || d.MipLevels == 0
        || d.Flags.0 & D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE.0 != 0 {
        return;
    }
    let hash = hash.to_owned();
    list.defer_host(move || {
        let frame = crate::frame::current();
        *DOME.lock().unwrap_or_else(|e| e.into_inner()) = Some(Dome {
            copy: copy.clone(), hash: hash.clone(), frame,
        });
        static SEEN: AtomicU64 = AtomicU64::new(0);
        let n = SEEN.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= 16 || n.is_power_of_two() {
            tracing::info!(%hash, frame, n, "L12 sky dome queued for submission");
        }
    });
}

// A group shares the 64-sample sky mean, rather than doing 64 loads
// for every output texel. Never read unbounded/NaN sky values.
const SOURCE: &str = r#"
#define RS "RootConstants(num32BitConstants=4, b0), DescriptorTable(UAV(u0), SRV(t0))"
cbuffer Params : register(b0) {
    float gain; float least; float power; float fallback;
};
RWTexture2D<uint> light : register(u0);
Texture2D<float4> dome : register(t0);
groupshared float4 samples[64];
groupshared float3 colour;
uint encode(float3 rgb) {
    rgb = clamp(rgb, 0.0, 65408.0);
    float peak = max(rgb.x, max(rgb.y,rgb.z));
    if (peak <= 0.0) return 0;
    int e = clamp(int(floor(log2(max(peak, exp2(-24.0)))))+16, 0, 31);
    float scale = exp2(float(24-e));
    if (peak*scale >= 511.5 && e<31) { ++e; scale *= 0.5; }
    uint3 m = min(uint3(rgb*scale+0.5),511u);
    return m.x | (m.y<<9) | (m.z<<18) | (uint(e)<<27);
}
[RootSignature(RS)]
[numthreads(8,8,1)]
void main(uint3 at: SV_DispatchThreadID, uint3 local: SV_GroupThreadID,
          uint i: SV_GroupIndex) {
    // 128x128 mip zero, centre of each 16x16 stratum.
    float3 sample_rgb = dome.Load(int3(local.xy*16+8,0)).rgb;
    bool valid = all(isfinite(sample_rgb));
    samples[i] = valid ? float4(clamp(sample_rgb,0.0,8.0),1.0) : float4(0,0,0,0);
    GroupMemoryBarrierWithGroupSync();
    if (i == 0) {
        float4 sum = 0.0;
        [loop] for (uint j=0; j<64; ++j) sum += samples[j];
        float3 mean = sum.rgb / max(sum.a,1.0);
        float m = dot(mean,float3(0.2126,0.7152,0.0722));
        float3 hue = lerp(float3(0.6,0.75,1.0),
            lerp(float3(1,1,1),mean/max(m,1e-6),0.5),saturate(m/0.05));
        colour = sum.a > 0.0 ? hue*(gain*pow(max(m,0.0),power)+least)
                             : float3(fallback,fallback,fallback);
    }
    GroupMemoryBarrierWithGroupSync();
    uint w,h;
    light.GetDimensions(w,h);
    if (at.x<w && at.y<h) light[at.xy] = encode(colour);
}
"#;

struct Pass {
    root: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}
// SAFETY: immutable D3D12 root signature and PSO, free-threaded.
unsafe impl Send for Pass {}
// SAFETY: as above.
unsafe impl Sync for Pass {}

fn make(device: &ID3D12Device) -> windows::core::Result<Pass> {
    let code = crate::host::compile(SOURCE, windows::core::s!("main"), windows::core::s!("cs_5_1"))?;
    // SAFETY: the compiled shader includes its root signature.
    unsafe {
        let root: ID3D12RootSignature = device.CreateRootSignature(0, &code)?;
        let pso = device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            CS: D3D12_SHADER_BYTECODE { pShaderBytecode: code.as_ptr().cast(), BytecodeLength: code.len() },
            ..Default::default()
        })?;
        Ok(Pass { root, pso })
    }
}

pub(crate) fn record(list: &HostList, target: &Target, level: f32) -> bool {
    if !enabled() { return false; }
    let Some([gain,least,power]) = settings() else { return false };
    let Some((copy, hash, sky_frame)) = DOME.lock().unwrap_or_else(|e| e.into_inner())
        .as_ref().map(|d| (d.copy.clone(),d.hash.clone(),d.frame)) else { return false };
    let Some(host) = crate::host::host() else { return false };
    static PASS: OnceLock<Option<Pass>> = OnceLock::new();
    let Some(pass) = PASS.get_or_init(|| make(&host.device)
        .inspect_err(|e| tracing::warn!(error = %e, "L12 sky ambient PSO failed")).ok()).as_ref()
        else { return false };
    let made: windows::core::Result<ID3D12DescriptorHeap> = unsafe {
        host.device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: 2,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
            NodeMask: 0,
        })
    };
    let Ok(heap) = made else { return false };
    let uav = D3D12_UNORDERED_ACCESS_VIEW_DESC {
        Format: DXGI_FORMAT_R32_UINT,
        ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
        ..Default::default()
    };
    let srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_R16G16B16A16_FLOAT,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV { MipLevels: 1, ..Default::default() },
        },
    };
    list.keep(&heap);
    list.keep(&target.texture);
    list.keep(&copy.texture);
    list.keep(&pass.root);
    list.keep(&pass.pso);
    // Use the actual held host copy, not the guest object's address.
    list.settle(&copy, 0x40 | 0x80);
    crate::list::ensure_state(list, target.title as crate::object::This, 0x8);
    barrier(list);
    let constants = [gain,least,power,level];
    // SAFETY: open list, two immutable views of retained resources.
    let (w,h) = unsafe {
        let at = heap.GetCPUDescriptorHandleForHeapStart();
        let step = host.device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
        host.device.CreateUnorderedAccessView(&target.texture, None::<&ID3D12Resource>, Some(&uav), at);
        host.device.CreateShaderResourceView(&copy.texture, Some(&srv),
            D3D12_CPU_DESCRIPTOR_HANDLE { ptr: at.ptr + step as usize });
        list.list.SetDescriptorHeaps(&[Some(heap.clone())]);
        list.list.SetComputeRootSignature(&pass.root);
        list.list.SetPipelineState(&pass.pso);
        list.list.SetComputeRoot32BitConstants(0, 4, constants.as_ptr().cast(), 0);
        list.list.SetComputeRootDescriptorTable(1, heap.GetGPUDescriptorHandleForHeapStart());
        let d = target.texture.GetDesc();
        let (w,h) = (d.Width as u32,d.Height);
        list.list.Dispatch(w.div_ceil(8),h.div_ceil(8),1);
        (w,h)
    };
    barrier(list);
    static RECORDED: AtomicU64 = AtomicU64::new(0);
    let n = RECORDED.fetch_add(1, Ordering::Relaxed) + 1;
    if n <= 16 || n.is_power_of_two() {
        tracing::info!(sky_hash = %hash, resolve_hash = %target.hash, sky_frame,
            frame = crate::frame::current(), gain, least, power, w, h, n,
            "L12 sky ambient recorded");
    }
    #[cfg(feature = "diagnostics")]
    list.note_op(|| format!("L12 sky ambient recorded sky {hash} resolve {} {w}x{h}",target.hash));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sky_opt_in_is_explicit_and_bounded() {
        assert_eq!(parse("1"),Some(DEFAULT));
        assert_eq!(parse("0.36,0.05,0.4"),Some(DEFAULT));
        for bad in ["","0","off","NaN,0.05,0.4","0.36,-1,0.4","0.36,0.05,0","1,2,3,4"] {
            assert_eq!(parse(bad),None);
        }
    }
    #[test]
    fn sky_pipeline_builds() {
        let Some(host) = crate::host::host() else { return };
        make(&host.device).expect("L12 sky ambient PSO");
    }
}
