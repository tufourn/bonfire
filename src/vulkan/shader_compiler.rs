use anyhow::{Context, Result};
use std::{cell::OnceCell, ffi::CStr, path::Path};

use shader_slang::{self as slang, Downcast};

use bytes::Bytes;

static SEARCH_PATH: &'static CStr = c"assets/shaders/slang";

thread_local! {
    static SLANG_GLOBAL_SESSION: OnceCell<slang::GlobalSession> = OnceCell::new();
}

pub fn with_slang_global_session<F, R>(f: F) -> R
where
    F: FnOnce(&slang::GlobalSession) -> R,
{
    SLANG_GLOBAL_SESSION.with(|cell| {
        let session =
            cell.get_or_init(|| slang::GlobalSession::new().expect("Failed to create session"));
        f(session)
    })
}

pub struct ShaderCompiler {}

impl ShaderCompiler {
    pub fn compile_slang<P>(path: P) -> Result<CompiledShader>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        with_slang_global_session(|global_session| {
            let compiler_options = slang::CompilerOptions::default()
                .emit_spirv_directly(true)
                .matrix_layout_row(true);
            let target_desc = slang::TargetDesc::default()
                .format(slang::CompileTarget::Spirv)
                .profile(global_session.find_profile("glsl_450"));

            let targets = [target_desc];
            let search_paths = [SEARCH_PATH.as_ptr()];
            let session_desc = slang::SessionDesc::default()
                .targets(&targets)
                .search_paths(&search_paths)
                .options(&compiler_options);

            let session = global_session
                .create_session(&session_desc)
                .context("Failed to create slang session")?;
            let module = session
                .load_module(&path.to_string_lossy())
                .context("Failed to load slang module")?;
            let entry_point = module
                .find_entry_point_by_name("main")
                .context("Failed to find entry point")?;

            let program = session
                .create_composite_component_type(&[
                    module.downcast().clone(),
                    entry_point.downcast().clone(),
                ])
                .context("Failed to create shader program")?;
            let linked_program = program.link().context("Failed to link shader program")?;

            let shader_bytecode = linked_program
                .entry_point_code(0, 0)
                .context("Failed to find entry point in shader")?;

            let shader_name = path
                .file_stem()
                .expect("Failed to get shader filename")
                .to_string_lossy()
                .to_string();

            let spirv = Bytes::copy_from_slice(shader_bytecode.as_slice());

            println!("Compiled {} ({} bytes)", shader_name, spirv.len());

            Ok(CompiledShader {
                name: shader_name,
                spirv,
            })
        })
    }
}

pub struct CompiledShader {
    pub name: String,
    pub spirv: Bytes,
}
