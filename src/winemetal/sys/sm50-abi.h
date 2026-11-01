/* The translator's C entry points (src/airconv/airconv_public.h) for winemetal-sys (src/winemetal/sys/src/sm50.rs),
 * with sm50_ptr64_t handles as uint64_t across every target. tools/winemetal-layouts.sh --seam-abi reads this block. */
#pragma once

/* dxmt-abi begin */
typedef uint64_t sm50_shader_t;
typedef uint64_t sm50_bitcode_t;
typedef uint64_t sm50_error_t;
struct MTL_SHADER_REFLECTION;
struct MTL_SM50_SHADER_ARGUMENT;
struct SM50_COMPILED_BITCODE;
struct SM50_SHADER_COMPILATION_ARGUMENT_DATA;

int SM50Initialize(
    const void *pBytecode, size_t BytecodeSize, sm50_shader_t *ppShader, struct MTL_SHADER_REFLECTION *pRefl,
    sm50_error_t *ppError
);
void SM50Destroy(sm50_shader_t pShader);
int SM50Compile(
    sm50_shader_t pShader, struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *pArgs, const char *FunctionName,
    sm50_bitcode_t *ppBitcode, sm50_error_t *ppError
);
void SM50GetCompiledBitcode(sm50_bitcode_t pBitcode, struct SM50_COMPILED_BITCODE *pData);
void SM50DestroyBitcode(sm50_bitcode_t pBitcode);
size_t SM50GetErrorMessage(sm50_error_t pError, char *pBuffer, size_t BufferSize);
void SM50FreeError(sm50_error_t pError);
int SM50CompileTessellationPipelineHull(
    sm50_shader_t pVertexShader, sm50_shader_t pHullShader, struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *pHullShaderArgs,
    const char *FunctionName, sm50_bitcode_t *ppBitcode, sm50_error_t *ppError
);
int SM50CompileTessellationPipelineDomain(
    sm50_shader_t pHullShader, sm50_shader_t pDomainShader,
    struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *pDomainShaderArgs, const char *FunctionName,
    sm50_bitcode_t *ppBitcode, sm50_error_t *ppError
);
int SM50CompileGeometryPipelineVertex(
    sm50_shader_t pVertexShader, sm50_shader_t pGeometryShader,
    struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *pVertexShaderArgs, const char *FunctionName, sm50_bitcode_t *ppBitcode,
    sm50_error_t *ppError
);
int SM50CompileGeometryPipelineGeometry(
    sm50_shader_t pVertexShader, sm50_shader_t pGeometryShader,
    struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *pGeometryShaderArgs, const char *FunctionName,
    sm50_bitcode_t *ppBitcode, sm50_error_t *ppError
);
void SM50GetArgumentsInfo(
    sm50_shader_t pShader, struct MTL_SM50_SHADER_ARGUMENT *pConstantBuffers, struct MTL_SM50_SHADER_ARGUMENT *pArguments
);
/* dxmt-abi end */
