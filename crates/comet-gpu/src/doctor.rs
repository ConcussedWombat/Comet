//! Read-only hardware checks; no game files or cache are required.

use anyhow::Result;

#[cfg(not(windows))]
pub fn run() -> Result<()> {
    anyhow::bail!("Comet's native D3D12 runtime requires Windows")
}

#[cfg(windows)]
pub fn run() -> Result<()> {
    use anyhow::{Context, ensure};
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::*;

    #[cfg(target_arch = "x86_64")]
    let avx2 = std::arch::is_x86_feature_detected!("avx2");
    #[cfg(not(target_arch = "x86_64"))]
    let avx2 = false;

    println!("Comet hardware check (selected D3D12 adapter)");
    println!("x86-64 AVX2: {}", yes_no(avx2));
    list_adapters();
    let device = create_selected_device().context("cannot create the selected D3D12 device")?;
    println!("D3D12 feature level 12.0: yes");

    let mut model = D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: D3D_SHADER_MODEL_6_6,
    };
    let mut mesh = D3D12_FEATURE_DATA_D3D12_OPTIONS7::default();
    let mut wave = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
    // SAFETY: DXGI returns owned interfaces; the feature queries write
    // into initialized structures of the size specified for each query.
    let (desc, sm_ok, mesh_ok, wave_ok) = unsafe {
        let desc = CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0))
            .and_then(|factory| factory.EnumAdapterByLuid::<IDXGIAdapter1>(device.GetAdapterLuid()))
            .and_then(|adapter| adapter.GetDesc1())
            .ok();
        let sm_ok = device
            .CheckFeatureSupport(
                D3D12_FEATURE_SHADER_MODEL,
                (&raw mut model).cast(),
                size_of_val(&model) as u32,
            )
            .is_ok();
        let mesh_ok = device
            .CheckFeatureSupport(
                D3D12_FEATURE_D3D12_OPTIONS7,
                (&raw mut mesh).cast(),
                size_of_val(&mesh) as u32,
            )
            .is_ok();
        let wave_ok = device
            .CheckFeatureSupport(
                D3D12_FEATURE_D3D12_OPTIONS1,
                (&raw mut wave).cast(),
                size_of_val(&wave) as u32,
            )
            .is_ok();
        (desc, sm_ok, mesh_ok, wave_ok)
    };

    let mut hardware = false;
    let mut nvidia = false;
    if let Some(desc) = desc {
        let end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        println!("Adapter: {}", String::from_utf16_lossy(&desc.Description[..end]));
        println!("Vendor: {:#06x}", desc.VendorId);
        println!("Dedicated video memory: {} MiB", desc.DedicatedVideoMemory / (1024 * 1024));
        hardware = desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0;
        nvidia = desc.VendorId == 0x10de;
        println!("Hardware adapter: {}", yes_no(hardware));
    } else {
        println!("Adapter identity: unavailable");
    }
    let sm66 = sm_ok && model.HighestShaderModel.0 >= D3D_SHADER_MODEL_6_6.0;
    let meshes = mesh_ok && mesh.MeshShaderTier != D3D12_MESH_SHADER_TIER_NOT_SUPPORTED;
    println!("Shader model 6.6: {}", yes_no(sm66));
    println!("Mesh shaders: {}", yes_no(meshes));
    if wave_ok {
        println!("Wave lanes: {}-{}", wave.WaveLaneCountMin, wave.WaveLaneCountMax);
    } else {
        println!("Wave lanes: unavailable");
    }
    if !nvidia {
        println!("Note: non-NVIDIA adapters are not validated for gameplay.");
    }
    ensure!(
        avx2 && hardware && sm66 && meshes,
        "the selected adapter or CPU does not meet the documented minimum requirements"
    );
    println!("Minimum requirements met; gameplay compatibility is not guaranteed.");
    Ok(())
}

#[cfg(windows)]
fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(windows)]
fn list_adapters() {
    use windows::Win32::Graphics::Dxgi::*;
    // SAFETY: adapter enumeration only reads DXGI descriptions.
    unsafe {
        if let Ok(factory) = CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0)) {
            let mut index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(index) {
                if let Ok(desc) = adapter.GetDesc1() {
                    let end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
                    let name = String::from_utf16_lossy(&desc.Description[..end]);
                    let software = desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0;
                    println!("DXGI adapter {index}: {name}{}", if software { " (software)" } else { "" });
                }
                index += 1;
            }
        }
    }
}

#[cfg(windows)]
fn adapter_index(value: &std::ffi::OsStr) -> Option<u32> {
    value.to_str()?.parse().ok()
}

/// Create the same device for the runtime and the hardware check. An
/// explicit index is a DXGI EnumAdapters1 index, not an adapter preference
/// ranking. An invalid selection fails instead of silently changing GPUs.
#[cfg(windows)]
pub(crate) fn create_selected_device() -> windows::core::Result<windows::Win32::Graphics::Direct3D12::ID3D12Device> {
    use windows::Win32::Foundation::E_INVALIDARG;
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::*;

    let mut device: Option<ID3D12Device> = None;
    // SAFETY: DXGI returns owned adapters and D3D12 writes an owned
    // device to the output parameter.
    unsafe {
        if let Some(value) = std::env::var_os("COMET_ADAPTER_INDEX") {
            let Some(index) = adapter_index(&value) else {
                tracing::error!("COMET_ADAPTER_INDEX must be a nonnegative DXGI adapter index");
                return Err(windows::core::Error::from_hresult(E_INVALIDARG));
            };
            let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let adapter = factory.EnumAdapters1(index)?;
            let desc = adapter.GetDesc1()?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                tracing::error!(index, "explicit adapter is software; refusing gameplay device");
                return Err(windows::core::Error::from_hresult(E_INVALIDARG));
            }
            D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device)?;
            tracing::info!(index, "selected explicit DXGI adapter");
        } else {
            D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device)?;
        }
    }
    device.ok_or_else(windows::core::Error::empty)
}

#[cfg(all(windows, test))]
mod tests {
    use super::adapter_index;
    use std::ffi::OsStr;

    #[test]
    fn adapter_index_is_explicit_and_unsigned() {
        assert_eq!(adapter_index(OsStr::new("0")), Some(0));
        assert_eq!(adapter_index(OsStr::new("2")), Some(2));
        for value in ["", "-1", "auto", "1.0", "4294967296"] {
            assert_eq!(adapter_index(OsStr::new(value)), None);
        }
    }
}
