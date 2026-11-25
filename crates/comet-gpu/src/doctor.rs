//! Read-only hardware checks; no game files or cache are required.

use anyhow::Result;

#[cfg(not(windows))]
pub fn run() -> Result<()> {
    anyhow::bail!("Comet's native D3D12 runtime requires Windows")
}

#[cfg(windows)]
pub fn run() -> Result<()> {
    use anyhow::{Context, ensure};
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::*;

    #[cfg(target_arch = "x86_64")]
    let avx2 = std::arch::is_x86_feature_detected!("avx2");
    #[cfg(not(target_arch = "x86_64"))]
    let avx2 = false;

    println!("Comet hardware check (default D3D12 adapter)");
    println!("x86-64 AVX2: {}", yes_no(avx2));
    let mut device: Option<ID3D12Device> = None;
    // SAFETY: D3D12 writes an owned device interface to the out parameter.
    unsafe { D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device) }
        .context("cannot create the default D3D12 feature-level 12.0 device")?;
    let device = device.context("D3D12 returned no device")?;
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
        "the default adapter or CPU does not meet the documented minimum requirements"
    );
    println!("Minimum requirements met; gameplay compatibility is not guaranteed.");
    Ok(())
}

#[cfg(windows)]
fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
