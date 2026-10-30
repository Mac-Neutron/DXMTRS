#pragma once
#include "../airconv_public.h"
// Compiles a vertex function whose outputs are exactly `PixelShader`'s inputs (same names and types, zeros, plus a
// position), so a pixel shader can be put in a Metal pipeline on its own (dxil-translate). Test use only.
#ifdef __cplusplus
extern "C" {
#endif
AIRCONV_API int DXILCompilePassThroughVertex(sm50_shader_t PixelShader, sm50_bitcode_t *ppBitcode, sm50_error_t *ppError);
#ifdef __cplusplus
}
#endif

/*
 * The test tools' view of a shader (dxil-translate): what DXILGetEntry answers, in C types only, so a tool needs no
 * other airconv header. Values are DXC's DxilConstants.h (DXIL's ShaderKind, SemanticKind and ResourceClass); the DXBC
 * program types D3D10_SB_PIXEL_SHADER..D3D11_SB_COMPUTE_SHADER share the values 0-5 of the shader kinds. Test use only.
 */
enum DXIL_SHADER_KIND {
  DXIL_SHADER_KIND_PIXEL = 0,
  DXIL_SHADER_KIND_VERTEX = 1,
  DXIL_SHADER_KIND_GEOMETRY = 2,
  DXIL_SHADER_KIND_HULL = 3,
  DXIL_SHADER_KIND_DOMAIN = 4,
  DXIL_SHADER_KIND_COMPUTE = 5,
};

/* Only the kinds a tool tests; any other value is DXC's SemanticKind number. */
enum DXIL_SEMANTIC {
  DXIL_SEMANTIC_ARBITRARY = 0,
  DXIL_SEMANTIC_POSITION = 3,
  DXIL_SEMANTIC_DEPTH = 17,
  DXIL_SEMANTIC_DEPTH_LESS_EQUAL = 18,
  DXIL_SEMANTIC_DEPTH_GREATER_EQUAL = 19,
  DXIL_SEMANTIC_STENCIL_REF = 20,
};

enum DXIL_RESOURCE_CLASS {
  DXIL_RESOURCE_CLASS_SRV = 0,
  DXIL_RESOURCE_CLASS_UAV = 1,
  DXIL_RESOURCE_CLASS_CBUFFER = 2,
  DXIL_RESOURCE_CLASS_SAMPLER = 3,
};

typedef struct {
  uint32_t kind;       /* a DXIL_SEMANTIC value */
  uint32_t rows;
  int32_t start_row;   /* -1: a system value that isn't packed */
  const char *name;    /* the semantic name, as the shader spells it */
  uint32_t num_semantic_indices;
  const uint32_t *semantic_indices; /* one per row; fewer than `rows` when the shader lists fewer */
} DXIL_ENTRY_ELEMENT;

typedef struct {
  uint32_t cls;        /* a DXIL_RESOURCE_CLASS value */
  uint32_t space, lower_bound, size, heap;
} DXIL_ENTRY_RESOURCE;

typedef struct {
  uint32_t is_dxil;
  uint32_t stage;      /* a DXIL_SHADER_KIND value */
  uint32_t num_inputs, num_outputs, num_resources;
  const DXIL_ENTRY_ELEMENT *inputs, *outputs;
  const DXIL_ENTRY_RESOURCE *resources;
} DXIL_ENTRY_INFO;

#ifdef __cplusplus
extern "C" {
#endif
/*
 * Fills `out` with a shader's entry point: the signature elements of a DXIL shader's entry (all of its resources), or
 * of a DXBC shader's ISGN (inputs) and OSGN (outputs; the rasterized stream), where `start_row` is the register,
 * `rows` 1, `semantic_indices` the one SemanticIndex, `kind` the system value's DXIL_SEMANTIC (a position, clip or
 * cull distance, array index, vertex, instance or primitive ID, front face or sample index; SV_Depth,
 * SV_DepthLessEqual, SV_DepthGreaterEqual and SV_StencilRef by their names) and DXIL_SEMANTIC_ARBITRARY for anything
 * else (a user varying; SV_Target and SV_Coverage too), with no resources. The pointers belong to the shader handle
 * until SM50Destroy.
 * Test use only.
 */
AIRCONV_API void DXILGetEntry(sm50_shader_t shader, DXIL_ENTRY_INFO *out);
#ifdef __cplusplus
}
#endif
