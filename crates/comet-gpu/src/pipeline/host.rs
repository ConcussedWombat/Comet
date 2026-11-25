//! Host pipelines for title pipelines (decision 0007, step 3): the
//! title's graphics state with the translated shaders, under a root
//! signature of our own that the translated shaders' bindings define
//! (`comet_shader::emit`): per stage one block of root constants for the
//! user data, root views for the title's root views, static samplers for
//! the immediate samplers, and directly indexed heaps.

use std::ffi::{CStr, CString};
use std::sync::Arc;

use comet_shader::emit::{ROOT_CBV_SPACE, SAMPLER_SPACE, USER_DATA_SPACE};
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::{Interface, PCSTR};

use super::{Shader, Translated};
use crate::host::host;

/// A stream's graphics state, owned (the title's stream lives only for the
/// creation call). Subobjects the stream leaves out keep D3D12's defaults.
pub(crate) struct GraphicsState {
    blend: D3D12_BLEND_DESC,
    sample_mask: u32,
    rasterizer: D3D12_RASTERIZER_DESC,
    depth: D3D12_DEPTH_STENCIL_DESC1,
    /// Input elements, their names pointing into `names`.
    inputs: Vec<D3D12_INPUT_ELEMENT_DESC>,
    names: Vec<CString>,
    strip_cut: D3D12_INDEX_BUFFER_STRIP_CUT_VALUE,
    pub(crate) topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE,
    pub(crate) targets: D3D12_RT_FORMAT_ARRAY,
    depth_format: DXGI_FORMAT,
    samples: DXGI_SAMPLE_DESC,
}

impl Default for GraphicsState {
    fn default() -> Self {
        let target = D3D12_RENDER_TARGET_BLEND_DESC {
            SrcBlend: D3D12_BLEND_ONE,
            DestBlend: D3D12_BLEND_ZERO,
            BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ONE,
            DestBlendAlpha: D3D12_BLEND_ZERO,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
            ..Default::default()
        };
        let keep = D3D12_DEPTH_STENCILOP_DESC {
            StencilFailOp: D3D12_STENCIL_OP_KEEP,
            StencilDepthFailOp: D3D12_STENCIL_OP_KEEP,
            StencilPassOp: D3D12_STENCIL_OP_KEEP,
            StencilFunc: D3D12_COMPARISON_FUNC_ALWAYS,
        };
        GraphicsState {
            blend: D3D12_BLEND_DESC {
                RenderTarget: [target; 8],
                ..Default::default()
            },
            sample_mask: u32::MAX,
            rasterizer: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_BACK,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            depth: D3D12_DEPTH_STENCIL_DESC1 {
                DepthEnable: true.into(),
                DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ALL,
                DepthFunc: D3D12_COMPARISON_FUNC_LESS,
                StencilReadMask: 0xff,
                StencilWriteMask: 0xff,
                FrontFace: keep,
                BackFace: keep,
                ..Default::default()
            },
            inputs: Vec::new(),
            names: Vec::new(),
            strip_cut: D3D12_INDEX_BUFFER_STRIP_CUT_VALUE_DISABLED,
            topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_UNDEFINED,
            targets: D3D12_RT_FORMAT_ARRAY::default(),
            depth_format: DXGI_FORMAT_UNKNOWN,
            samples: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
        }
    }
}

/// Exact shader hashes for diagnostic rasterizer overrides, not prefixes.
fn r4_geometry_hashes(value: &str) -> Vec<String> {
    value.split(',').map(str::trim)
        .filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase).collect()
}

fn r4_geometry_selected(shaders: &[Arc<Shader>], hashes: &[String]) -> bool {
    shaders.iter().any(|s| matches!(s.stage, "vertex" | "mesh") && hashes.contains(&s.hash))
}

impl GraphicsState {
    /// Copy the complete state, including owned semantic names, so the
    /// shared title PSO description and its pointers are never mutated.
    fn r4_probe_copy(&self, no_cull: bool, wireframe: bool) -> Self {
        let names = self.names.clone();
        let mut inputs = self.inputs.clone();
        for (e, name) in inputs.iter_mut().zip(&names) {
            e.SemanticName = PCSTR(name.as_ptr().cast());
        }
        let mut rasterizer = self.rasterizer;
        if no_cull {
            rasterizer.CullMode = D3D12_CULL_MODE_NONE;
        }
        if wireframe {
            rasterizer.FillMode = D3D12_FILL_MODE_WIREFRAME;
        }
        Self {
            blend: self.blend,
            sample_mask: self.sample_mask,
            rasterizer,
            depth: self.depth,
            inputs,
            names,
            strip_cut: self.strip_cut,
            topology: self.topology,
            targets: self.targets,
            depth_format: self.depth_format,
            samples: self.samples,
        }
    }
    /// Takes in one subobject of a stream: its type, then its data at +4
    /// (+8 for those holding pointers).
    ///
    /// # Safety
    /// `at` points at a whole subobject of type `kind`.
    pub(crate) unsafe fn read(&mut self, kind: u32, at: *const u8) {
        // SAFETY: the caller passes a whole subobject.
        unsafe {
            let data = at.add(4);
            match kind {
                8 => self.blend = data.cast::<D3D12_BLEND_DESC>().read_unaligned(),
                9 => self.sample_mask = data.cast::<u32>().read_unaligned(),
                10 => self.rasterizer = data.cast::<D3D12_RASTERIZER_DESC>().read_unaligned(),
                11 => {
                    let d = data.cast::<D3D12_DEPTH_STENCIL_DESC>().read_unaligned();
                    self.depth = D3D12_DEPTH_STENCIL_DESC1 {
                        DepthEnable: d.DepthEnable,
                        DepthWriteMask: d.DepthWriteMask,
                        DepthFunc: d.DepthFunc,
                        StencilEnable: d.StencilEnable,
                        StencilReadMask: d.StencilReadMask,
                        StencilWriteMask: d.StencilWriteMask,
                        FrontFace: d.FrontFace,
                        BackFace: d.BackFace,
                        DepthBoundsTestEnable: false.into(),
                    };
                }
                21 => {
                    let d = data.cast::<D3D12_DEPTH_STENCIL_DESC1>().read_unaligned();
                    if d.DepthBoundsTestEnable.as_bool() && !r4_depth_bounds_enabled() {
                        static SEEN: std::sync::atomic::AtomicU64 =
                            std::sync::atomic::AtomicU64::new(0);
                        if SEEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 4 {
                            tracing::warn!(
                                "a pipeline enables the depth bounds test, which its host pipeline leaves out: its draws pass at every depth"
                            );
                        }
                    }
                    self.depth = d;
                }
                12 => {
                    let layout = at.add(8).cast::<D3D12_INPUT_LAYOUT_DESC>().read_unaligned();
                    if layout.pInputElementDescs.is_null() {
                        return;
                    }
                    let elements = std::slice::from_raw_parts(
                        layout.pInputElementDescs,
                        layout.NumElements as usize,
                    );
                    self.names = elements
                        .iter()
                        .map(|e| CStr::from_ptr(e.SemanticName.0.cast()).to_owned())
                        .collect();
                    self.inputs = elements
                        .iter()
                        .zip(&self.names)
                        .map(|(e, name)| D3D12_INPUT_ELEMENT_DESC {
                            SemanticName: PCSTR(name.as_ptr().cast()),
                            Format: DXGI_FORMAT(
                                crate::resource::pc_format(e.Format.0 as u32) as i32
                            ),
                            ..*e
                        })
                        .collect();
                }
                13 => {
                    self.strip_cut =
                        D3D12_INDEX_BUFFER_STRIP_CUT_VALUE(data.cast::<i32>().read_unaligned())
                }
                14 => {
                    self.topology =
                        D3D12_PRIMITIVE_TOPOLOGY_TYPE(data.cast::<i32>().read_unaligned())
                }
                15 => {
                    self.targets = data.cast::<D3D12_RT_FORMAT_ARRAY>().read_unaligned();
                    for f in &mut self.targets.RTFormats {
                        f.0 = crate::resource::target_format(crate::resource::pc_format(
                            f.0 as u32,
                        )) as i32;
                    }
                }
                16 => {
                    let f = data.cast::<u32>().read_unaligned();
                    self.depth_format = DXGI_FORMAT(crate::resource::pc_format(f) as i32);
                }
                17 => self.samples = data.cast::<DXGI_SAMPLE_DESC>().read_unaligned(),
                _ => {}
            }
        }
    }

    /// Everything the state holds, as text: part of a pipeline's name in
    /// the library and of the key a title pipeline is started early by.
    pub(crate) fn describe(&self) -> String {
        let inputs: Vec<String> = self
            .inputs
            .iter()
            .zip(&self.names)
            .map(|(e, name)| {
                format!(
                    "{name:?} {} {} {} {} {} {}",
                    e.SemanticIndex, e.Format.0, e.InputSlot, e.AlignedByteOffset, e.InputSlotClass.0, e.InstanceDataStepRate
                )
            })
            .collect();
        let bounds = if self.depth.DepthBoundsTestEnable.as_bool() && r4_depth_bounds_enabled() {
            " R4 depth bounds"
        } else {
            ""
        };
        format!(
            "{:?} {} {:?} {:?}{bounds} {inputs:?} {} {} {:?} {} {:?}",
            self.blend,
            self.sample_mask,
            self.rasterizer,
            self.r4_depth0(),
            self.strip_cut.0,
            self.topology.0,
            self.targets,
            self.depth_format.0,
            self.samples
        )
    }

    fn r4_depth0(&self) -> D3D12_DEPTH_STENCIL_DESC {
        let d = &self.depth;
        D3D12_DEPTH_STENCIL_DESC {
            DepthEnable: d.DepthEnable,
            DepthWriteMask: d.DepthWriteMask,
            DepthFunc: d.DepthFunc,
            StencilEnable: d.StencilEnable,
            StencilReadMask: d.StencilReadMask,
            StencilWriteMask: d.StencilWriteMask,
            FrontFace: d.FrontFace,
            BackFace: d.BackFace,
        }
    }

    /// The vertex buffer slots the input layout reads.
    fn slots(&self) -> u32 {
        self.inputs
            .iter()
            .fold(0, |m, e| m | 1 << e.InputSlot.min(31))
    }

    /// The input elements with their byte offsets worked out.
    fn elements(&self) -> Vec<Input> {
        let mut next = [0u32; 32];
        self.inputs
            .iter()
            .map(|e| {
                let slot = e.InputSlot.min(31) as usize;
                let format = e.Format.0 as u32;
                let offset = if e.AlignedByteOffset == D3D12_APPEND_ALIGNED_ELEMENT {
                    next[slot]
                } else {
                    e.AlignedByteOffset
                };
                let bytes = crate::resource::element(format).map_or(0, |(b, _)| b as u32);
                next[slot] = offset + bytes;
                Input {
                    slot: slot as u32,
                    offset,
                    format,
                    per_instance: e.InputSlotClass == D3D12_INPUT_CLASSIFICATION_PER_INSTANCE_DATA,
                }
            })
            .collect()
    }
}

/// One input element: vertex buffer slot, byte offset, format.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Input {
    pub(crate) slot: u32,
    pub(crate) offset: u32,
    pub(crate) format: u32,
    pub(crate) per_instance: bool,
}

/// The console's own primitive topology types.
const RECTANGLES: i32 = 19;
const QUADS: i32 = 32;

/// Whether the title's root SRVs are bound as one-view tables of the
/// buffer's host copy, whose reads past its end return zeros, rather than
/// as root descriptors, which have no size: a mesh shader's vertex fetch
/// has no record limit, and a read past the copy reaches whatever the host
/// placed after it (memory it has freed: the device is removed, upstream
/// PR #493). `COMET_ROOT_SRV_DIRECT=1` goes back to root descriptors.
pub(crate) fn bounded_root_srvs() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_ROOT_SRV_DIRECT").is_none())
}

/// One stage of a host pipeline and where its bindings go.
pub(crate) struct Stage {
    pub(crate) shader: Arc<Shader>,
    /// The root parameter of its user data, and its size in dwords.
    pub(crate) user_data: Option<(u32, u32)>,
    /// Root parameters of its root views: (title root parameter, ours).
    pub(crate) root_cbvs: Vec<(u8, u32)>,
    pub(crate) root_srvs: Vec<(u8, u32)>,
    /// Root UAVs: one-view tables like the SRVs (or root descriptors with
    /// `COMET_ROOT_SRV_DIRECT=1`).
    pub(crate) root_uavs: Vec<(u8, u32)>,
}

impl Stage {
    pub(crate) fn translated(&self) -> &Translated {
        self.shader
            .translated()
            .expect("host pipelines are made from translated shaders")
    }
}

pub(crate) struct HostPipeline {
    /// A compute pipeline (else graphics).
    pub(crate) compute: bool,
    /// A mesh pipeline (graphics without vertex input).
    pub(crate) mesh: bool,
    pub(crate) pso: ID3D12PipelineState,
    pub(crate) root: ID3D12RootSignature,
    /// Vertex, then pixel stage, or the compute stage.
    pub(crate) stages: Vec<Stage>,
    /// The vertex buffer slots it reads (bit per slot).
    pub(crate) slots: u32,
    pub(crate) inputs: Vec<Input>,
    /// Has the geometry stage that completes the console's rectangles
    /// (`comet_shader::emit::rect_list_gs`): its rectangle draws go out
    /// as the title gives them, three vertices a rectangle.
    pub(crate) rects: bool,
    /// Its shaders, for reports of slow submissions.
    pub(crate) name: String,
    /// Its depth, stencil, cull and blend state in short, for the frame
    /// listing (empty for compute).
    #[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
    pub(crate) state: String,
}

/// Opt-in diagnosis and repair only for R4's known refused PSOs.
fn r4_pso_selected(shaders: &[Arc<Shader>]) -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R4_PSO").is_none_or(|v| v != "0"))
        && shaders.iter().any(|s| matches!(s.hash.as_str(),
            "02384a60fda6f2d0" | "2cc4c197ee29e291" | "2da4beb386509b77" |
            "369cbe906ea72764" | "f5e50eb2108c425b" | "f133e35efed2a48a" |
            "6b6db9f1a526daeb"))
}

/// Unused render-target slots are not part of the title's active state.
/// D3D12 requires UNKNOWN in the slots beyond NumRenderTargets.
fn r4_targets(mut targets: D3D12_RT_FORMAT_ARRAY) -> D3D12_RT_FORMAT_ARRAY {
    for f in targets.RTFormats.iter_mut().skip(targets.NumRenderTargets as usize) {
        *f = DXGI_FORMAT_UNKNOWN;
    }
    targets
}

/// Validate a private copy; do not mutate the cached translation or use
/// bytes from a failed validator call. The library keys the actual bytes.
fn r4_validated(shader: &Shader, bytes: &[u8], keep_unchanged: bool) -> Option<Vec<u8>> {
    use comet_shader::emit::validator::Validator;
    let mut validator = match Validator::load(Validator::default_path().as_deref()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(hash = %shader.hash, error = %e, "R4 DXIL validator unavailable");
            return None;
        }
    };
    let mut copy = bytes.to_vec();
    match validator.validate(&mut copy) {
        Ok(()) => {
            let changed = copy != bytes;
            tracing::info!(hash = %shader.hash, changed, "R4 DXIL validated");
            (changed || keep_unchanged).then_some(copy)
        }
        Err(e) => {
            tracing::warn!(hash = %shader.hash, error = %e, "R4 DXIL validation failed");
            None
        }
    }
}

/// Additional retries on the already-landed R4 PSO path, never on all PSOs.
fn r4_retry_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R4_PSO_RETRY").is_some_and(|v| v == "1"))
}

fn r4_invalidarg<T>(result: &windows::core::Result<T>) -> bool {
    result.as_ref().is_err_and(|e| e.code() == windows::Win32::Foundation::E_INVALIDARG)
}

/// Use the equivalent pipeline stream creation API for a refused compute PSO.
/// It describes the same root signature and bytecode, with default node mask
/// and flags. A validator success is required; no fallback shader is used.
fn r4_compute_stream(
    device: &ID3D12Device,
    root: &ID3D12RootSignature,
    cs: D3D12_SHADER_BYTECODE,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    #[repr(C, align(8))]
    struct Sub<T>(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE, T);
    #[repr(C)]
    struct Stream {
        root: Sub<*mut std::ffi::c_void>,
        cs: Sub<D3D12_SHADER_BYTECODE>,
    }
    let mut stream = Stream {
        root: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_ROOT_SIGNATURE, root.as_raw()),
        cs: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_CS, cs),
    };
    let desc = D3D12_PIPELINE_STATE_STREAM_DESC {
        SizeInBytes: size_of::<Stream>(),
        pPipelineStateSubobjectStream: (&raw mut stream).cast(),
    };
    let device = device.cast::<ID3D12Device2>()?;
    super::library::pipeline(
        name,
        // SAFETY: stream and referenced bytes remain live for each call.
        |library, name| unsafe { library.LoadPipeline(name, &desc) },
        || unsafe { device.CreatePipelineState(&desc) },
    )
}

/// Canonicalize blend entries not used by any active target.
fn r4_inactive_blend(g: &GraphicsState) -> D3D12_BLEND_DESC {
    let mut blend = g.blend;
    let first = if blend.IndependentBlendEnable.as_bool() {
        g.targets.NumRenderTargets.min(8) as usize
    } else {
        1
    };
    blend.RenderTarget[first..].fill(GraphicsState::default().blend.RenderTarget[0]);
    blend
}

/// Logs a pipeline that gets no host pipeline, once per reason.
fn refuse(reason: &'static str, shaders: &[Arc<Shader>]) -> Option<HostPipeline> {
    static SEEN: std::sync::Mutex<Vec<&str>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.contains(&reason) {
        seen.push(reason);
        tracing::warn!(?shaders, "no host pipeline: {reason} (logged once)");
    }
    None
}

impl HostPipeline {
    /// A host pipeline for vertex and pixel shaders with the graphics
    /// state `g`, or for one compute shader.
    pub(crate) fn new(shaders: &[Arc<Shader>], g: Option<&GraphicsState>) -> Option<HostPipeline> {
        static CULL: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        static WIRE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        static STATE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        let state_hashes = STATE.get_or_init(|| r4_geometry_hashes(
            &std::env::var("COMET_R4_STATE_HASHES").unwrap_or_default()));
        let selected_state = shaders.iter().any(|s| state_hashes.contains(&s.hash));
        let cull = CULL.get_or_init(|| r4_geometry_hashes(
            &std::env::var("COMET_R4_CULL_PROBE").unwrap_or_default()));
        let wire = WIRE.get_or_init(|| r4_geometry_hashes(
            &std::env::var("COMET_R4_WIREFRAME").unwrap_or_default()));
        let trace = *TRACE.get_or_init(|| std::env::var_os("COMET_R4_GEOMETRY_TRACE").is_some_and(|v| v == "1"));
        let probe = g.and_then(|g| {
            let no_cull = r4_geometry_selected(shaders, cull);
            let mut wireframe = r4_geometry_selected(shaders, wire);
            if wireframe && g.rasterizer.ConservativeRaster != D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF {
                tracing::warn!(?shaders, "R4 wireframe not applied to conservative rasterization");
                wireframe = false;
            }
            if !no_cull && !wireframe {
                return None;
            }
            let state = g.r4_probe_copy(no_cull, wireframe);
            tracing::info!(?shaders, no_cull, wireframe,
                original_cull = g.rasterizer.CullMode.0,
                original_fill = g.rasterizer.FillMode.0,
                front_ccw = g.rasterizer.FrontCounterClockwise.as_bool(),
                "R4 geometry rasterizer probe");
            Some(state)
        });
        let effective = probe.as_ref().or(g);
        // All creation paths, including either R4 PSO recovery alternative,
        // receive the same effective descriptor and cache identity.
        let mut made = Self::new_inner(shaders, effective)?;
        if let Some(g) = effective
            && ((made.mesh && (trace || probe.is_some())) || selected_state)
        {
            made.state = g.describe();
        }
        if (trace && made.mesh) || selected_state {
            tracing::info!(?shaders, state = %made.state, "R4 geometry PSO state");
        }
        Some(made)
    }

    fn new_inner(shaders: &[Arc<Shader>], g: Option<&GraphicsState>) -> Option<HostPipeline> {
        // For the report when the host device is removed: what this thread
        // has in the library from here on.
        super::library::label(format!("{shaders:?}"));
        let find = |stage| shaders.iter().find(|s| s.stage == stage);
        let compute = match shaders {
            [cs] if cs.stage == "compute" => Some(cs),
            _ => None,
        };
        if compute.is_none()
            && shaders
                .iter()
                .any(|s| !matches!(s.stage, "vertex" | "pixel" | "mesh"))
        {
            return refuse(
                "only vertex, mesh, pixel and compute stages are translated",
                shaders,
            );
        }
        let mesh = find("mesh");
        let stages = if let Some(cs) = compute {
            vec![(cs, D3D12_SHADER_VISIBILITY_ALL)]
        } else {
            let first = match (mesh, find("vertex")) {
                (Some(ms), _) => (ms, D3D12_SHADER_VISIBILITY_MESH),
                (None, Some(vs)) => (vs, D3D12_SHADER_VISIBILITY_VERTEX),
                (None, None) => return refuse("no vertex or mesh shader", shaders),
            };
            let mut stages = vec![first];
            stages.extend(find("pixel").map(|ps| (ps, D3D12_SHADER_VISIBILITY_PIXEL)));
            stages
        };
        if mesh.is_some() && !host()?.mesh_shaders() {
            return refuse("the host GPU has no mesh shaders (or no shader model 6.6)", shaders);
        }
        for (s, _) in &stages {
            if r4_wave_selected(s) {
                r4_wave_translation(s)?;
            } else {
                s.translated()?;
            }
        }
        let host = host()?;

        // The root signature the translated shaders' bindings define.
        let mut params: Vec<D3D12_ROOT_PARAMETER1> = Vec::new();
        // The ranges table parameters point at, alive until the root
        // signature is made.
        let mut ranges: Vec<Box<D3D12_DESCRIPTOR_RANGE1>> = Vec::new();
        let mut samplers = Vec::new();
        let mut bound = Vec::new();
        for &(shader, visibility) in &stages {
            let t = &shader.translated()?.host;
            let mut stage = Stage {
                shader: shader.clone(),
                user_data: None,
                root_cbvs: Vec::new(),
                root_srvs: Vec::new(),
                root_uavs: Vec::new(),
            };
            let dwords = t.user_data_dwords();
            if dwords > 0 {
                stage.user_data = Some((params.len() as u32, dwords));
                params.push(D3D12_ROOT_PARAMETER1 {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                    Anonymous: D3D12_ROOT_PARAMETER1_0 {
                        Constants: D3D12_ROOT_CONSTANTS {
                            ShaderRegister: 0,
                            RegisterSpace: USER_DATA_SPACE,
                            Num32BitValues: dwords,
                        },
                    },
                    ShaderVisibility: visibility,
                });
            }
            for (slots, kind, out) in [
                (
                    &t.root_cbvs,
                    D3D12_ROOT_PARAMETER_TYPE_CBV,
                    &mut stage.root_cbvs,
                ),
                (
                    &t.root_srvs,
                    D3D12_ROOT_PARAMETER_TYPE_SRV,
                    &mut stage.root_srvs,
                ),
                (
                    &t.root_uavs,
                    D3D12_ROOT_PARAMETER_TYPE_UAV,
                    &mut stage.root_uavs,
                ),
            ] {
                for &slot in slots {
                    let uav = kind == D3D12_ROOT_PARAMETER_TYPE_UAV;
                    // Ours, not the title's, stay root descriptors: the
                    // table of strides, the directory of title memory by
                    // address and its fault notes.
                    let ours = if uav {
                        slot == comet_shader::emit::SLOT_FAULTS
                    } else {
                        slot == comet_shader::emit::SLOT_STRIDES
                            || slot == comet_shader::emit::SLOT_MEMORY
                    };
                    if (uav || kind == D3D12_ROOT_PARAMETER_TYPE_SRV) && !ours && bounded_root_srvs() {
                        // A one-view table: the view's size bounds the
                        // reads (and a root UAV's writes), which a root
                        // descriptor leaves unbounded.
                        // (Our own table of strides stays a descriptor.)
                        ranges.push(Box::new(D3D12_DESCRIPTOR_RANGE1 {
                            RangeType: if uav {
                                D3D12_DESCRIPTOR_RANGE_TYPE_UAV
                            } else {
                                D3D12_DESCRIPTOR_RANGE_TYPE_SRV
                            },
                            NumDescriptors: 1,
                            BaseShaderRegister: u32::from(slot),
                            RegisterSpace: ROOT_CBV_SPACE,
                            Flags: D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE
                                | D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE,
                            OffsetInDescriptorsFromTableStart: 0,
                        }));
                        let range: *const D3D12_DESCRIPTOR_RANGE1 = &**ranges.last()?;
                        out.push((slot, params.len() as u32));
                        params.push(D3D12_ROOT_PARAMETER1 {
                            ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                            Anonymous: D3D12_ROOT_PARAMETER1_0 {
                                DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE1 {
                                    NumDescriptorRanges: 1,
                                    pDescriptorRanges: range,
                                },
                            },
                            ShaderVisibility: visibility,
                        });
                        continue;
                    }
                    out.push((slot, params.len() as u32));
                    params.push(D3D12_ROOT_PARAMETER1 {
                        ParameterType: kind,
                        Anonymous: D3D12_ROOT_PARAMETER1_0 {
                            Descriptor: D3D12_ROOT_DESCRIPTOR1 {
                                ShaderRegister: u32::from(slot),
                                RegisterSpace: ROOT_CBV_SPACE,
                                Flags: D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE,
                            },
                        },
                        ShaderVisibility: visibility,
                    });
                }
            }
            for (n, s) in t.static_samplers.iter().enumerate() {
                samplers.push(D3D12_STATIC_SAMPLER_DESC {
                    Filter: D3D12_FILTER(s.filter as i32),
                    AddressU: D3D12_TEXTURE_ADDRESS_MODE(s.address[0] as i32),
                    AddressV: D3D12_TEXTURE_ADDRESS_MODE(s.address[1] as i32),
                    AddressW: D3D12_TEXTURE_ADDRESS_MODE(s.address[2] as i32),
                    MipLODBias: s.mip_lod_bias,
                    MaxAnisotropy: s.max_anisotropy,
                    ComparisonFunc: D3D12_COMPARISON_FUNC(s.comparison as i32),
                    BorderColor: D3D12_STATIC_BORDER_COLOR(s.border as i32),
                    MinLOD: s.min_lod,
                    MaxLOD: s.max_lod,
                    ShaderRegister: n as u32,
                    RegisterSpace: SAMPLER_SPACE,
                    ShaderVisibility: visibility,
                });
            }
            bound.push(stage);
        }
        let (root, root_blob) = match root_signature(&host.device, &params, &samplers, compute.is_some()) {
            Ok(root) => root,
            Err(e) => {
                tracing::warn!(error = %e, ?shaders, "host root signature refused");
                return None;
            }
        };
        let r4 = r4_pso_selected(shaders);
        let mut validated = Vec::new();
        if r4 {
            for (shader, _) in &stages {
                let t = shader.translated()?;
                tracing::info!(
                    hash = %shader.hash, stage = shader.stage,
                    dwords = t.host.user_data_dwords(),
                    cbvs = ?t.host.root_cbvs, srvs = ?t.host.root_srvs,
                    uavs = ?t.host.root_uavs, samplers = t.host.static_samplers.len(),
                    used = ?t.used, "R4 PSO bindings"
                );
                validated.push(r4_validated(shader, &t.host.dxil, r4_retry_enabled()));
            }
        }
        // Original shader bytes or a validator-signed private copy, alive
        // until PSO creation returns. Both library and driver see the same bytes.
        let code_bytes = |i: usize| {
            validated.get(i).and_then(Option::as_deref).unwrap_or_else(|| {
                stages.get(i).and_then(|(s, _)| s.translated())
                    .map_or(&[][..], |t| t.host.dxil.as_slice())
            })
        };
        let code = |i: usize| {
            let bytes = code_bytes(i);
            if bytes.is_empty() {
                D3D12_SHADER_BYTECODE::default()
            } else {
                D3D12_SHADER_BYTECODE {
                    pShaderBytecode: bytes.as_ptr().cast(),
                    BytecodeLength: bytes.len(),
                }
            }
        };
        if compute.is_some() {
            let desc = D3D12_COMPUTE_PIPELINE_STATE_DESC {
                // SAFETY: a borrowed pointer the description never releases.
                pRootSignature: unsafe { std::mem::transmute_copy(&root) },
                CS: code(0),
                ..Default::default()
            };
            let name = super::library::name("compute", &root_blob, &[code_bytes(0)], "");
            let mut made = super::library::pipeline(
                &name,
                // SAFETY: the description points at live data.
                |library, name| unsafe { library.LoadComputePipeline(name, &desc) },
                // SAFETY: as above.
                || unsafe { host.device.CreateComputePipelineState(&desc) },
            );
            // The landed path has already validated/signed the bytecode.
            // A retry is eligible only if validation succeeded and the
            // original driver creation refused the same description.
            if r4 && r4_retry_enabled() && r4_invalidarg(&made)
                && validated.first().is_some_and(Option::is_some)
            {
                host.log_messages();
                let name = super::library::name("R4 compute stream validated", &root_blob, &[code_bytes(0)], "");
                made = r4_compute_stream(&host.device, &root, code(0), &name);
                tracing::info!(?shaders, accepted = made.is_ok(), "R4 compute stream PSO retry");
            }
            let pso = match made {
                Ok(pso) => pso,
                Err(e) => {
                    tracing::warn!(error = %e, ?shaders, "host pipeline refused");
                    host.log_messages();
                    return None;
                }
            };
            tracing::info!(?shaders, "host pipeline");
            crate::host::name(&pso, || format!("{shaders:?}"));
            return Some(HostPipeline {
                compute: true,
                mesh: false,
                pso,
                root,
                stages: bound,
                name: format!("{shaders:?}"),
                slots: 0,
                inputs: Vec::new(),
                rects: false,
                state: String::new(),
            });
        }
        let Some(g) = g else {
            return refuse("graphics state not kept (plain description)", shaders);
        };
        let mut state = g.describe();
        if r4 {
            tracing::info!(?shaders, %state, "R4 graphics PSO description");
            if g.targets.NumRenderTargets > 8 {
                return refuse("R4 graphics pipeline has more than eight targets", shaders);
            }
            // Give canonicalized descriptions a separate persistent name.
            state.push_str(" R4 canonical RTV tail");
        }
        let targets = if r4 { r4_targets(g.targets) } else { g.targets };
        let bounds = g.depth.DepthBoundsTestEnable.as_bool() && r4_depth_bounds_supported(&host.device);
        if g.depth.DepthBoundsTestEnable.as_bool() && r4_depth_bounds_enabled() {
            tracing::info!(?shaders, enabled = bounds, "R4 depth-bounds PSO");
        }
        if mesh.is_some() {
            // "triangles": the stream names its topology since it did not
            // load from the library without one (see `mesh_pipeline`); a
            // new name keeps such entries, stored before, from being
            // looked up again.
            let name = super::library::name(
                "mesh triangles",
                &root_blob,
                &[code_bytes(0), code_bytes(1)],
                &state,
            );
            let mut made = mesh_pipeline(&host.device, &root, code(0), code(1), (g, targets, g.blend), &name);
            if r4 && r4_retry_enabled() && r4_invalidarg(&made)
                && validated.len() == stages.len() && validated.iter().all(Option::is_some)
            {
                host.log_messages();
                let blend = r4_inactive_blend(g);
                let name = super::library::name("R4 mesh canonical blend", &root_blob, &[code_bytes(0), code_bytes(1)], &state);
                made = mesh_pipeline(&host.device, &root, code(0), code(1), (g, targets, blend), &name);
                tracing::info!(?shaders, accepted = made.is_ok(), "R4 mesh PSO retry");
            }
            let pso = match made {
                Ok(pso) => pso,
                Err(e) => {
                    tracing::warn!(error = %e, ?shaders, "host mesh pipeline refused");
                    host.log_messages();
                    return None;
                }
            };
            tracing::info!(?shaders, "host mesh pipeline");
            crate::host::name(&pso, || format!("{shaders:?}"));
            return Some(HostPipeline {
                compute: false,
                mesh: true,
                pso,
                root,
                stages: bound,
                name: format!("{shaders:?}"),
                slots: 0,
                inputs: Vec::new(),
                rects: false,
                state: String::new(),
            });
        }

        // A pipeline the title makes for rectangles gets a geometry stage
        // that completes each one from the three vertices the vertex
        // shader made. Without it (refused here or by the host) the draws
        // fall back on what `draw` does for rectangles.
        let mut rect_gs = None;
        if g.topology.0 == RECTANGLES {
            let ps = stages.get(1).and_then(|(s, _)| s.translated());
            let vs = &stages[0].0.translated()?.host.dxil;
            match comet_shader::emit::rect_list_gs(vs, ps.map(|t| &t.host.dxil[..])) {
                Ok(gs) => rect_gs = Some(gs),
                Err(e) => tracing::warn!(error = %e, ?shaders, "no rectangle geometry stage"),
            }
        }
        let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            // SAFETY: a borrowed pointer the description never releases.
            pRootSignature: unsafe { std::mem::transmute_copy(&root) },
            VS: code(0),
            PS: code(1),
            GS: rect_gs.as_ref().map_or(D3D12_SHADER_BYTECODE::default(), |gs| {
                D3D12_SHADER_BYTECODE {
                    pShaderBytecode: gs.as_ptr().cast(),
                    BytecodeLength: gs.len(),
                }
            }),
            BlendState: g.blend,
            SampleMask: g.sample_mask,
            RasterizerState: g.rasterizer,
            DepthStencilState: g.r4_depth0(),
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: g.inputs.as_ptr(),
                NumElements: g.inputs.len() as u32,
            },
            IBStripCutValue: g.strip_cut,
            // The console's rectangle and quad types draw as triangles on
            // the host (see `draw`).
            PrimitiveTopologyType: match g.topology.0 {
                RECTANGLES | QUADS => D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                _ => g.topology,
            },
            NumRenderTargets: targets.NumRenderTargets,
            RTVFormats: targets.RTFormats,
            DSVFormat: g.depth_format,
            SampleDesc: g.samples,
            ..Default::default()
        };
        // A pipeline with the geometry stage has a name of its own in the
        // library; the others keep theirs.
        let name = |gs: Option<&Vec<u8>>| match gs {
            Some(gs) => super::library::name(
                "graphics rectangles",
                &root_blob,
                &[code_bytes(0), code_bytes(1), &gs[..]],
                &state,
            ),
            None => super::library::name(
                "graphics",
                &root_blob,
                &[code_bytes(0), code_bytes(1)],
                &state,
            ),
        };
        let make = |desc: &D3D12_GRAPHICS_PIPELINE_STATE_DESC, name: &str| {
            if bounds {
                return r4_graphics_depth_bounds_pipeline(&host.device, &root, desc, g.depth, name);
            }
            super::library::pipeline(
                name,
                // SAFETY: the description points at live data.
                |library, name| unsafe { library.LoadGraphicsPipeline(name, desc) },
                // SAFETY: as above.
                || unsafe { host.device.CreateGraphicsPipelineState(desc) },
            )
        };
        let mut made = make(&desc, &name(rect_gs.as_ref()));
        if made.is_err() && rect_gs.is_some() {
            // The host refused the geometry stage: once more without it.
            tracing::warn!(?shaders, "host pipeline refused with the rectangle geometry stage");
            host.log_messages();
            desc.GS = D3D12_SHADER_BYTECODE::default();
            rect_gs = None;
            made = make(&desc, &name(None));
        }
        let pso = match made {
            Ok(pso) => pso,
            Err(e) => {
                tracing::warn!(error = %e, ?shaders, "host pipeline refused");
                host.log_messages();
                return None;
            }
        };
        tracing::info!(?shaders, "host pipeline");
        crate::host::name(&pso, || format!("{shaders:?}"));
        tracing::debug!(
            ?shaders,
            depth = ?g.depth,
            dsv = g.depth_format.0,
            targets = ?&g.targets.RTFormats[..g.targets.NumRenderTargets.min(8) as usize],
            blend = ?g.blend.RenderTarget[0],
            cull = g.rasterizer.CullMode.0,
            "host graphics state"
        );
        Some(HostPipeline {
            compute: false,
            mesh: false,
            pso,
            root,
            stages: bound,
            name: format!("{shaders:?}"),
            slots: g.slots(),
            inputs: g.elements(),
            rects: rect_gs.is_some(),
            state: {
                let (d, r, b) = (&g.depth, &g.rasterizer, &g.blend.RenderTarget[0]);
                let face = |f: &D3D12_DEPTH_STENCILOP_DESC| {
                    format!(
                        "fail {} depth-fail {} pass {} func {}",
                        f.StencilFailOp.0, f.StencilDepthFailOp.0, f.StencilPassOp.0, f.StencilFunc.0
                    )
                };
                format!(
                    "depth {} write {} func {}; stencil {} read {:#x} write {:#x} front [{}] back [{}]; cull {} front-ccw {}; blend {} src {} dst {} op {} mask {:#x}",
                    d.DepthEnable.as_bool(),
                    d.DepthWriteMask.0,
                    d.DepthFunc.0,
                    d.StencilEnable.as_bool(),
                    d.StencilReadMask,
                    d.StencilWriteMask,
                    face(&d.FrontFace),
                    face(&d.BackFace),
                    r.CullMode.0,
                    r.FrontCounterClockwise.as_bool(),
                    b.BlendEnable.as_bool(),
                    b.SrcBlend.0,
                    b.DestBlend.0,
                    b.BlendOp.0,
                    b.RenderTargetWriteMask
                )
            },
        })
    }
}

// Depth bounds adapted from Kaivyyy (#82), fxceless/victory-fork
// c96b4f2b8365bf43d54ab074940923ef8267f4c6 (GPL-3.0-or-later).
pub(crate) fn r4_depth_bounds_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R4_DEPTH_BOUNDS").is_some_and(|v| v == "1"))
}

pub(crate) fn r4_depth_bounds_supported(device: &ID3D12Device) -> bool {
    if !r4_depth_bounds_enabled() {
        return false;
    }
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS2::default();
        // SAFETY: valid feature structure with its exact size.
        let ok = unsafe {
            device.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS2,
                (&raw mut options).cast(), size_of_val(&options) as u32).is_ok()
        };
        let supported = ok && options.DepthBoundsTestSupported.as_bool();
        tracing::info!(supported, "R4 host depth bounds support");
        supported
    })
}

/// A stream subobject: its type, then its description, pointer-aligned.
#[repr(C, align(8))]
struct Sub<T>(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE, T);

fn r4_bounds_stream_pipeline<S>(
    device: &ID3D12Device,
    stream: &mut S,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    let desc = D3D12_PIPELINE_STATE_STREAM_DESC {
        SizeInBytes: size_of::<S>(),
        pPipelineStateSubobjectStream: std::ptr::from_mut(stream).cast(),
    };
    let device = device.cast::<ID3D12Device2>()?;
    super::library::pipeline(
        name,
        // SAFETY: the stream holds whole subobjects pointing at live data.
        |library, name| unsafe { library.LoadPipeline(name, &desc) },
        // SAFETY: as above.
        || unsafe { device.CreatePipelineState(&desc) },
    )
}

/// A graphics pipeline enabling the depth bounds test: `D3D12_GRAPHICS_PIPELINE_STATE_DESC`
/// has no field for `DepthBoundsTestEnable`, so it is created and loaded as a
/// pipeline-state stream with `DEPTH_STENCIL1`.
fn r4_graphics_depth_bounds_pipeline(
    device: &ID3D12Device,
    root: &ID3D12RootSignature,
    d: &D3D12_GRAPHICS_PIPELINE_STATE_DESC,
    depth: D3D12_DEPTH_STENCIL_DESC1,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    #[repr(C)]
    struct Stream {
        root: Sub<*mut std::ffi::c_void>,
        vs: Sub<D3D12_SHADER_BYTECODE>,
        ps: Sub<D3D12_SHADER_BYTECODE>,
        blend: Sub<D3D12_BLEND_DESC>,
        mask: Sub<u32>,
        raster: Sub<D3D12_RASTERIZER_DESC>,
        depth: Sub<D3D12_DEPTH_STENCIL_DESC1>,
        input: Sub<D3D12_INPUT_LAYOUT_DESC>,
        strip_cut: Sub<D3D12_INDEX_BUFFER_STRIP_CUT_VALUE>,
        topology: Sub<D3D12_PRIMITIVE_TOPOLOGY_TYPE>,
        targets: Sub<D3D12_RT_FORMAT_ARRAY>,
        dsv: Sub<DXGI_FORMAT>,
        samples: Sub<DXGI_SAMPLE_DESC>,
    }
    #[repr(C)]
    struct StreamGs {
        base: Stream,
        gs: Sub<D3D12_SHADER_BYTECODE>,
    }
    let targets = D3D12_RT_FORMAT_ARRAY {
        RTFormats: d.RTVFormats,
        NumRenderTargets: d.NumRenderTargets,
    };
    let base = Stream {
        root: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_ROOT_SIGNATURE, root.as_raw()),
        vs: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_VS, d.VS),
        ps: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_PS, d.PS),
        blend: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_BLEND, d.BlendState),
        mask: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_MASK, d.SampleMask),
        raster: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RASTERIZER, d.RasterizerState),
        depth: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL1, depth),
        input: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_INPUT_LAYOUT, d.InputLayout),
        strip_cut: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_IB_STRIP_CUT_VALUE, d.IBStripCutValue),
        topology: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY, d.PrimitiveTopologyType),
        targets: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RENDER_TARGET_FORMATS, targets),
        dsv: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT, d.DSVFormat),
        samples: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_DESC, d.SampleDesc),
    };
    if d.GS.pShaderBytecode.is_null() {
        let mut stream = base;
        r4_bounds_stream_pipeline(device, &mut stream, name)
    } else {
        let mut stream = StreamGs {
            base,
            gs: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_GS, d.GS),
        };
        r4_bounds_stream_pipeline(device, &mut stream, name)
    }
}

/// A mesh pipeline from the title's graphics state: the pipeline-state
/// stream form (no input layout).
///
/// The stream names a triangle topology, which D3D12 does not need for a
/// mesh shader: without it the pipeline library refuses to load a stored
/// pipeline with the very stream it was made from (`E_INVALIDARG`; debug
/// layer message 972, "does not match the one used to create the PSO"), so
/// every mesh pipeline was compiled again, and its store then refused (970,
/// "already exists"). The translation makes triangle mesh shaders only
/// (`mesh_prologue` refuses others), and the debug layer wants the two to
/// agree (1323).
fn mesh_pipeline(
    device: &ID3D12Device,
    root: &ID3D12RootSignature,
    ms: D3D12_SHADER_BYTECODE,
    ps: D3D12_SHADER_BYTECODE,
    (g, targets, blend): (&GraphicsState, D3D12_RT_FORMAT_ARRAY, D3D12_BLEND_DESC),
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    use windows::core::Interface;
    /// A stream subobject: its type, then its description, pointer-aligned.
    #[repr(C, align(8))]
    struct Sub<T>(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE, T);
    #[repr(C)]
    struct Stream<D> {
        root: Sub<*mut std::ffi::c_void>,
        ms: Sub<D3D12_SHADER_BYTECODE>,
        ps: Sub<D3D12_SHADER_BYTECODE>,
        blend: Sub<D3D12_BLEND_DESC>,
        mask: Sub<u32>,
        raster: Sub<D3D12_RASTERIZER_DESC>,
        depth: Sub<D>,
        targets: Sub<D3D12_RT_FORMAT_ARRAY>,
        dsv: Sub<DXGI_FORMAT>,
        samples: Sub<DXGI_SAMPLE_DESC>,
        topology: Sub<D3D12_PRIMITIVE_TOPOLOGY_TYPE>,
    }
    let mut stream = Stream {
        root: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_ROOT_SIGNATURE, root.as_raw()),
        ms: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_MS, ms),
        ps: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_PS, ps),
        blend: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_BLEND, blend),
        mask: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_MASK, g.sample_mask),
        raster: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RASTERIZER, g.rasterizer),
        depth: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL, g.r4_depth0()),
        targets: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RENDER_TARGET_FORMATS, targets),
        dsv: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT, g.depth_format),
        samples: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_DESC, g.samples),
        topology: Sub(
            D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY,
            D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
        ),
    };
    if g.depth.DepthBoundsTestEnable.as_bool() && r4_depth_bounds_supported(device) {
        let mut bounded = Stream {
            root: stream.root,
            ms: stream.ms,
            ps: stream.ps,
            blend: stream.blend,
            mask: stream.mask,
            raster: stream.raster,
            depth: Sub(D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL1, g.depth),
            targets: stream.targets,
            dsv: stream.dsv,
            samples: stream.samples,
            topology: stream.topology,
        };
        return r4_bounds_stream_pipeline(device, &mut bounded, name);
    }
    let desc = D3D12_PIPELINE_STATE_STREAM_DESC {
        SizeInBytes: size_of_val(&stream),
        pPipelineStateSubobjectStream: (&raw mut stream).cast(),
    };
    let device = device.cast::<ID3D12Device2>()?;
    super::library::pipeline(
        name,
        // SAFETY: the stream holds whole subobjects pointing at live data.
        |library, name| unsafe { library.LoadPipeline(name, &desc) },
        // SAFETY: as above.
        || unsafe { device.CreatePipelineState(&desc) },
    )
}

/// The cache key for normal translations does not include COMET_R4_WAVE.
/// Never publish this experimental translation to that disk cache.
fn r4_wave_selected(shader: &Shader) -> bool {
    shader.stage == "compute" && shader.hash == "f133e35efed2a48a"
        && std::env::var_os("COMET_R4_WAVE").is_none_or(|v| v != "0")
}

fn r4_wave_translation(shader: &Shader) -> Option<&Translated> {
    shader.translated.get_or_init(|| {
        // The ordinary translation is always preferred, including a
        // successful repair by another worker. This closure is only
        // installed for the one known hash on the opt-in path.
        let done = super::translate(&shader.container).or_else(|original| {
            if !original.contains("0x1eb4: a lane mask exchanged where not every lane runs") {
                return Err(original);
            }
            use comet_shader::{Container, FourCC};
            use comet_shader::emit::validator::Validator;
            let c = Container::parse(&shader.container).map_err(|e| e.to_string())?;
            let part = c.part(FourCC::DXIL).ok_or("R4 missing DXIL part")?;
            let programs = comet_shader::native_programs(part.data).map_err(|e| e.to_string())?;
            let stores = programs.is_empty() || programs.iter().any(super::stores);
            let used = programs.into_iter()
                .find(|p| p.stage == comet_shader::HwStage::Cs)
                .ok_or("R4 missing compute program")?.used;
            let mut host = comet_shader::emit::translate_r4_exact(&shader.container)
                .map_err(|e| e.to_string())?;
            let mut validator = Validator::load(Validator::default_path().as_deref())?;
            validator.validate(&mut host.dxil)?;
            tracing::info!(hash = %shader.hash, "R4 exact wave retry validated");
            Ok(Translated { host, used, stores })
        });
        if let Some(dir) = std::env::var_os("COMET_SHADER_SAVE") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            let stem = format!("{}.{}", shader.hash, shader.stage);
            let _ = std::fs::write(dir.join(format!("{stem}.cdxil")), &shader.container);
            if let Ok(t) = &done {
                let _ = std::fs::write(dir.join(format!("{stem}.dxil")), &t.host.dxil);
            }
        }
        match &done {
            Ok(t) => {
                use std::sync::atomic::Ordering::Relaxed;
                super::KEPT[2].fetch_add(1, Relaxed);
                super::KEPT[3].fetch_add(t.host.dxil.len() as u64, Relaxed);
                tracing::info!(hash = %shader.hash, "R4 shader translated");
            }
            Err(e) => tracing::warn!(hash = %shader.hash, error = %e, "R4 exact wave retry refused"),
        }
        done.ok()
    }).as_ref()
}

/// Identical serialized root signatures are interchangeable on one
/// D3D12 device. Sharing them avoids driver allocation/validation during
/// pipeline prewarm and allows consecutive PSOs to retain root bindings.
/// Cap retained identities per shard; a full shard still creates valid
/// uncached signatures. Do not hold a cache lock inside the driver.
fn shared_root(
    device: &ID3D12Device,
    bytes: &[u8],
) -> windows::core::Result<ID3D12RootSignature> {
    type Key = (usize, Vec<u8>);
    type Cache = std::collections::HashMap<Key, ID3D12RootSignature>;
    static ROOTS: [std::sync::Mutex<Option<Cache>>; 16] =
        [const { std::sync::Mutex::new(None) }; 16];
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    // SAFETY: bytes are a complete serialized root signature.
    let create = || unsafe { device.CreateRootSignature::<ID3D12RootSignature>(0, bytes) };
    if !*ENABLED.get_or_init(|| std::env::var_os("COMET_SHARED_ROOTS").is_none_or(|v| v != "0")) {
        return create();
    }
    let key = (device.as_raw() as usize, bytes.to_vec());
    let hash = blake3::hash(bytes);
    let shard = &ROOTS[usize::from(hash.as_bytes()[0]) % ROOTS.len()];
    {
        let cache = shard.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(root) = cache.as_ref().and_then(|cache| cache.get(&key)) {
            return Ok(root.clone());
        }
    }
    let root = create()?;
    let mut cache = shard.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(Default::default);
    if let Some(existing) = cache.get(&key) {
        return Ok(existing.clone());
    }
    if cache.len() < 128 {
        // A root signature retains its device, so the device pointer
        // cannot be reused while an entry for it is alive.
        cache.insert(key, root.clone());
    }
    Ok(root)
}

fn root_signature(
    device: &ID3D12Device,
    params: &[D3D12_ROOT_PARAMETER1],
    samplers: &[D3D12_STATIC_SAMPLER_DESC],
    compute: bool,
) -> windows::core::Result<(ID3D12RootSignature, Vec<u8>)> {
    let input = if compute {
        D3D12_ROOT_SIGNATURE_FLAG_NONE
    } else {
        D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
    };
    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: windows::Win32::Graphics::Direct3D12::D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: params.len() as u32,
                pParameters: params.as_ptr(),
                NumStaticSamplers: samplers.len() as u32,
                pStaticSamplers: samplers.as_ptr(),
                Flags: input
                    | D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
                    | D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED,
            },
        },
    };
    // SAFETY: the description points at live arrays; the blob is read in
    // full before it is released.
    unsafe {
        let mut blob: Option<ID3DBlob> = None;
        let mut error: Option<ID3DBlob> = None;
        if let Err(e) = D3D12SerializeVersionedRootSignature(&desc, &mut blob, Some(&mut error)) {
            if let Some(error) = error {
                let text = std::slice::from_raw_parts(
                    error.GetBufferPointer().cast::<u8>(),
                    error.GetBufferSize(),
                );
                tracing::warn!("root signature: {}", String::from_utf8_lossy(text));
            }
            return Err(e);
        }
        let blob = blob.ok_or_else(windows::core::Error::empty)?;
        let bytes =
            std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
        Ok((shared_root(device, bytes)?, bytes.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn r4_retry_preserves_active_blend_entries() {
        let mut g = GraphicsState::default();
        g.targets.NumRenderTargets = 3;
        g.blend.IndependentBlendEnable = true.into();
        for target in &mut g.blend.RenderTarget {
            target.RenderTargetWriteMask = 3;
        }
        let independent = r4_inactive_blend(&g);
        assert_eq!(independent.RenderTarget[0].RenderTargetWriteMask, 3);
        assert_eq!(independent.RenderTarget[2].RenderTargetWriteMask, 3);
        assert_eq!(independent.RenderTarget[3].RenderTargetWriteMask,
            GraphicsState::default().blend.RenderTarget[3].RenderTargetWriteMask);
        g.blend.IndependentBlendEnable = false.into();
        let common = r4_inactive_blend(&g);
        assert_eq!(common.RenderTarget[0].RenderTargetWriteMask, 3);
        assert_eq!(common.RenderTarget[1].RenderTargetWriteMask,
            GraphicsState::default().blend.RenderTarget[1].RenderTargetWriteMask);
    }

    #[test]
    fn r4_inactive_targets_only() {
        for count in 0..=8 {
            let targets = D3D12_RT_FORMAT_ARRAY {
                RTFormats: [DXGI_FORMAT_R8G8B8A8_UNORM; 8],
                NumRenderTargets: count,
            };
            let out = r4_targets(targets);
            assert_eq!(out.NumRenderTargets, count);
            for (i, format) in out.RTFormats.iter().enumerate() {
                assert_eq!(*format, if i < count as usize {
                    DXGI_FORMAT_R8G8B8A8_UNORM
                } else {
                    DXGI_FORMAT_UNKNOWN
                });
            }
        }
    }

    #[test]
    fn identical_serialized_root_signatures_share_the_host_object() {
        let Some(host) = host() else {
            return;
        };
        let (a, a_blob) = root_signature(&host.device, &[], &[], false).unwrap();
        let (b, b_blob) = root_signature(&host.device, &[], &[], false).unwrap();
        let (c, c_blob) = root_signature(&host.device, &[], &[], true).unwrap();
        assert_eq!(a_blob, b_blob);
        assert_ne!(a_blob, c_blob);
        if std::env::var_os("COMET_SHARED_ROOTS").is_none_or(|v| v != "0") {
            assert_eq!(a.as_raw(), b.as_raw());
        }
        assert_ne!(a.as_raw(), c.as_raw());
    }

    #[test]
    fn r4_cull_probe_preserves_other_rasterizer_fields() {
        let mut g = GraphicsState::default();
        g.rasterizer.FrontCounterClockwise = true.into();
        g.rasterizer.DepthBias = 42;
        g.rasterizer.DepthClipEnable = false.into();
        g.names.push(CString::new("POSITION").unwrap());
        g.inputs.push(D3D12_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(g.names[0].as_ptr().cast()),
            Format: DXGI_FORMAT_R32G32B32_FLOAT,
            ..Default::default()
        });
        let probe = g.r4_probe_copy(true, false);
        let wire = g.r4_probe_copy(false, true);
        assert_eq!(probe.rasterizer.CullMode, D3D12_CULL_MODE_NONE);
        assert_eq!(probe.rasterizer.FillMode, g.rasterizer.FillMode);
        assert_eq!(wire.rasterizer.FillMode, D3D12_FILL_MODE_WIREFRAME);
        assert_eq!(wire.rasterizer.CullMode, g.rasterizer.CullMode);
        assert_eq!(probe.rasterizer.FrontCounterClockwise, g.rasterizer.FrontCounterClockwise);
        assert_eq!(probe.rasterizer.DepthBias, g.rasterizer.DepthBias);
        assert_eq!(probe.rasterizer.DepthClipEnable, g.rasterizer.DepthClipEnable);
        assert_eq!(probe.inputs[0].SemanticName.0, probe.names[0].as_ptr().cast());
        assert_ne!(probe.inputs[0].SemanticName.0, g.inputs[0].SemanticName.0);
        assert_eq!(probe.names[0], g.names[0]);
        assert_ne!(probe.describe(), g.describe());
        assert_ne!(wire.describe(), g.describe());
        assert_eq!(r4_geometry_hashes(" 0123456789ABCDEF, 0123, * "), ["0123456789abcdef"]);
    }

    /// A triangle mesh shader of our own and a pixel shader for it.
    const MS: &str = "struct V { float4 p : SV_Position; };
        [outputtopology(\"triangle\")] [numthreads(1, 1, 1)]
        void main(out vertices V v[3], out indices uint3 t[1]) {
            SetMeshOutputCounts(3, 1);
            v[0].p = float4(0, 0, 0, 1);
            v[1].p = float4(1, 0, 0, 1);
            v[2].p = float4(0, 1, 0, 1);
            t[0] = uint3(0, 1, 2);
        }";
    const PS: &str = "float4 main() : SV_Target { return 1; }";

    #[test]
    fn r4_depth_bounds_descriptor_preserves_depth_state() {
        let mut g = GraphicsState::default();
        #[repr(C, align(8))]
        struct Sub21(u32, D3D12_DEPTH_STENCIL_DESC1);
        let sub = Sub21(21, D3D12_DEPTH_STENCIL_DESC1 {
            DepthEnable: true.into(),
            DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ZERO,
            DepthFunc: D3D12_COMPARISON_FUNC_GREATER_EQUAL,
            DepthBoundsTestEnable: true.into(),
            ..Default::default()
        });
        // SAFETY: complete aligned subobject 21.
        unsafe { g.read(21, (&raw const sub).cast()) };
        assert!(g.depth.DepthBoundsTestEnable.as_bool());
        let depth = g.r4_depth0();
        assert_eq!(depth.DepthWriteMask, D3D12_DEPTH_WRITE_MASK_ZERO);
        assert_eq!(depth.DepthFunc, D3D12_COMPARISON_FUNC_GREATER_EQUAL);
        #[repr(C, align(8))]
        struct Sub11(u32, D3D12_DEPTH_STENCIL_DESC);
        let plain = Sub11(11, depth);
        // SAFETY: complete aligned subobject 11.
        unsafe { g.read(11, (&raw const plain).cast()) };
        assert!(!g.depth.DepthBoundsTestEnable.as_bool());
    }

    /// The PC `dxc.exe` (`COMET_PC_DXC`, else the newest Windows SDK's).
    fn dxc() -> Option<PathBuf> {
        let dxc = match std::env::var_os("COMET_PC_DXC") {
            Some(p) => PathBuf::from(p),
            None => comet_shader::emit::validator::Validator::default_path()?
                .with_file_name("dxc.exe"),
        };
        dxc.exists().then_some(dxc)
    }

    fn compile(dxc: &Path, dir: &Path, src: &str, profile: &str) -> Vec<u8> {
        let (hlsl, out) = (
            dir.join(format!("{profile}.hlsl")),
            dir.join(format!("{profile}.dxil")),
        );
        std::fs::write(&hlsl, src).unwrap();
        let status = std::process::Command::new(dxc)
            .args(["-nologo", "-T", profile, "-E", "main", "-Fo"])
            .arg(&out)
            .arg(&hlsl)
            .status()
            .unwrap();
        assert!(status.success(), "dxc failed for {profile}");
        std::fs::read(out).unwrap()
    }

    fn code(bytes: &[u8]) -> D3D12_SHADER_BYTECODE {
        D3D12_SHADER_BYTECODE {
            pShaderBytecode: bytes.as_ptr().cast(),
            BytecodeLength: bytes.len(),
        }
    }

    /// A mesh pipeline made once is loaded from the library the next time,
    /// not compiled again (a stream without a topology never loads).
    #[test]
    fn a_mesh_pipeline_loads_from_the_library() {
        let Some(host) = host() else {
            return; // no host GPU here
        };
        if !host.mesh_shaders() {
            eprintln!("no host mesh shader support; skipped");
            return;
        }
        let Some(dxc) = dxc() else {
            eprintln!("no PC dxc.exe; skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let ms = compile(&dxc, dir.path(), MS, "ms_6_5");
        let ps = compile(&dxc, dir.path(), PS, "ps_6_5");
        let (root, root_blob) = root_signature(&host.device, &[], &[], false).unwrap();
        let mut g = GraphicsState::default();
        // This pipeline has only a color target, with no depth buffer.
        g.depth.DepthEnable = false.into();
        g.targets.NumRenderTargets = 1;
        g.targets.RTFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
        let name = super::super::library::name("mesh test", &root_blob, &[&ms, &ps], &g.describe());
        mesh_pipeline(&host.device, &root, code(&ms), code(&ps), (&g, g.targets, g.blend), &name).expect("made");
        let loaded = super::super::library::LOADED.load(Ordering::Relaxed);
        mesh_pipeline(&host.device, &root, code(&ms), code(&ps), (&g, g.targets, g.blend), &name).expect("made again");
        assert!(
            super::super::library::LOADED.load(Ordering::Relaxed) > loaded,
            "the mesh pipeline was compiled again instead of loaded from the library"
        );
    }
}
