# The translator is Rust (this folder's crate airconv, which winemetal's static library holds: src/winemetal/unix's and
# src/nativemetal's custom targets build it with cargo, and its build.rs compiles the Metal helpers). Meson builds none
# of it: it passes airconv's headers to the C and C++ that call it, and the LLVM libraries below to the images that
# link it, which build.rs also reads (keep the list's form).
# AIRExtras (our fork's C entry points for what llvm-c lacks), then what llvm-config --libs airbitwriter passes linker
# bitreader printed
llvm_deps = [
  '-lLLVMAIRExtras',
  '-lLLVMPasses', '-lLLVMIRPrinter', '-lLLVMHipStdPar',
  '-lLLVMCoroutines', '-lLLVMipo', '-lLLVMInstrumentation',
  '-lLLVMVectorize', '-lLLVMSandboxIR', '-lLLVMLinker',
  '-lLLVMFrontendOpenMP', '-lLLVMFrontendDirective', '-lLLVMFrontendAtomic',
  '-lLLVMFrontendOffloading', '-lLLVMObjectYAML', '-lLLVMGlobalISel',
  '-lLLVMSelectionDAG', '-lLLVMCodeGen', '-lLLVMTarget',
  '-lLLVMScalarOpts', '-lLLVMInstCombine', '-lLLVMObjCARCOpts',
  '-lLLVMCodeGenTypes', '-lLLVMCGData', '-lLLVMBitWriter',
  '-lLLVMCFGuard', '-lLLVMAggressiveInstCombine', '-lLLVMTransformUtils',
  '-lLLVMAnalysis', '-lLLVMProfileData', '-lLLVMSymbolize',
  '-lLLVMDebugInfoBTF', '-lLLVMDebugInfoPDB', '-lLLVMDebugInfoMSF',
  '-lLLVMDebugInfoCodeView', '-lLLVMDebugInfoGSYM', '-lLLVMDebugInfoDWARF',
  '-lLLVMObject', '-lLLVMTextAPI', '-lLLVMMCParser',
  '-lLLVMIRReader', '-lLLVMAsmParser', '-lLLVMMC',
  '-lLLVMDebugInfoDWARFLowLevel', '-lLLVMBitReader', '-lLLVMFrontendHLSL',
  '-lLLVMAIRBitWriter', '-lLLVMCore', '-lLLVMRemarks',
  '-lLLVMBinaryFormat', '-lLLVMTargetParser', '-lLLVMBitstreamReader',
  '-lLLVMSupport', '-lLLVMDemangle'
]

airconv_forward_dep = declare_dependency(
  include_directories : [ dxmt_include_path, include_directories('.') ],
)

if (cpu_family == 'x86_64' or cpu_family == 'aarch64' or not dxmt_crossbuild) and not get_option('guest_vm')
  subdir('darwin')
endif
