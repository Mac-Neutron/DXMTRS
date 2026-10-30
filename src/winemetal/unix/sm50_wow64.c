/*
 * winemetal's WoW64 converters for the translator thunks (the Wine build only): the 32-bit params of the SM50*
 * calls widened to the 64-bit ones, named by the WoW64 table (src/winemetal/unix/src/table.rs).
 */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "../airconv_thunks.h"

typedef int NTSTATUS;
#define STATUS_SUCCESS 0

static inline void *
UInt32ToPtr(uint32_t v) {
  return (void *)(uint64_t)v;
}

#ifndef DXMT_NATIVE

NTSTATUS
thunk32_SM50Initialize(void *args) {
  struct sm50_initialize_params32 *params = args;

  params->ret = SM50Initialize(
      UInt32ToPtr(params->bytecode), params->bytecode_size, UInt32ToPtr(params->shader),
      UInt32ToPtr(params->reflection), UInt32ToPtr(params->error)
  );

  return STATUS_SUCCESS;
}

struct SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  uint32_t num_output_slots;
  uint32_t num_elements;
  uint32_t strides[4];
  uint32_t elements;
};

struct SM50_SHADER_COMMON_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  enum SM50_SHADER_METAL_VERSION metal_version;
  enum SM50_SHADER_FLAG flag;
};

struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
};

struct SM50_SHADER_IA_INPUT_LAYOUT_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  enum SM50_INDEX_BUFFER_FORMAT index_buffer_format;
  uint32_t slot_mask;
  uint32_t num_elements;
  uint32_t elements;
};

struct SM50_SHADER_PSO_PIXEL_SHADER_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  uint32_t sample_mask;
  bool dual_source_blending;
  bool disable_depth_output;
  uint32_t unorm_output_reg_mask;
  /** MTLPixelFormat */
  uint32_t pixel_formats[8];
};

struct SM50_SHADER_GS_PASS_THROUGH_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  union {
    struct MTL_GEOMETRY_SHADER_PASS_THROUGH Data;
    uint32_t DataEncoded;
  };
  bool RasterizationDisabled;
};

struct SM50_SHADER_PSO_GEOMETRY_SHADER_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  bool strip_topology;
};

struct SM50_SHADER_PSO_TESSELLATOR_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  uint32_t max_potential_tess_factor;
};

struct SM50_SHADER_ROOT_SIGNATURE_DATA32 {
  uint32_t next;
  enum SM50_SHADER_COMPILATION_ARGUMENT_TYPE type;
  uint32_t bytecode;
  uint32_t bytecode_length;
};

void
sm50_compilation_argument32_convert(
    struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *first_arg, struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 *args32
) {
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *last_arg = first_arg;

  first_arg->type = SM50_SHADER_ARGUMENT_TYPE_MAX;
  first_arg->next = NULL;

  while (args32) {
    switch (args32->type) {
    case SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT: {
      struct SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT_DATA32 *src = (void *)args32;
      struct SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT_DATA *data =
          malloc(sizeof(struct SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->num_output_slots = src->num_output_slots;
      data->num_elements = src->num_elements;
      data->strides[0] = src->strides[0];
      data->strides[1] = src->strides[1];
      data->strides[2] = src->strides[2];
      data->strides[3] = src->strides[3];
      data->elements = UInt32ToPtr(src->elements);
      break;
    }
    case SM50_SHADER_COMMON: {
      struct SM50_SHADER_COMMON_DATA32 *src = (void *)args32;
      struct SM50_SHADER_COMMON_DATA *data = malloc(sizeof(struct SM50_SHADER_COMMON_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->metal_version = src->metal_version;
      data->flags = src->flag;
      break;
    }
    case SM50_SHADER_PSO_PIXEL_SHADER: {
      struct SM50_SHADER_PSO_PIXEL_SHADER_DATA32 *src = (void *)args32;
      struct SM50_SHADER_PSO_PIXEL_SHADER_DATA *data = malloc(sizeof(struct SM50_SHADER_PSO_PIXEL_SHADER_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->unorm_output_reg_mask = src->unorm_output_reg_mask;
      data->disable_depth_output = src->disable_depth_output;
      data->sample_mask = src->sample_mask;
      data->dual_source_blending = src->dual_source_blending;
      memcpy(data->pixel_formats, src->pixel_formats, sizeof(data->pixel_formats));
      break;
    }
    case SM50_SHADER_IA_INPUT_LAYOUT: {
      struct SM50_SHADER_IA_INPUT_LAYOUT_DATA32 *src = (void *)args32;
      struct SM50_SHADER_IA_INPUT_LAYOUT_DATA *data = malloc(sizeof(struct SM50_SHADER_IA_INPUT_LAYOUT_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->slot_mask = src->slot_mask;
      data->index_buffer_format = src->index_buffer_format;
      data->num_elements = src->num_elements;
      data->elements = UInt32ToPtr(src->elements);
      break;
    }
    case SM50_SHADER_GS_PASS_THROUGH: {
      struct SM50_SHADER_GS_PASS_THROUGH_DATA32 *src = (void *)args32;
      struct SM50_SHADER_GS_PASS_THROUGH_DATA *data = malloc(sizeof(struct SM50_SHADER_GS_PASS_THROUGH_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->Data = src->Data;
      data->RasterizationDisabled = src->RasterizationDisabled;
      break;
    }
    case SM50_SHADER_PSO_GEOMETRY_SHADER: {
      struct SM50_SHADER_PSO_GEOMETRY_SHADER_DATA32 *src = (void *)args32;
      struct SM50_SHADER_PSO_GEOMETRY_SHADER_DATA *data = malloc(sizeof(struct SM50_SHADER_PSO_GEOMETRY_SHADER_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->strip_topology = src->strip_topology;
      break;
    }
    case SM50_SHADER_PSO_TESSELLATOR: {
      struct SM50_SHADER_PSO_TESSELLATOR_DATA32 *src = (void *)args32;
      struct SM50_SHADER_PSO_TESSELLATOR_DATA *data = malloc(sizeof(struct SM50_SHADER_PSO_TESSELLATOR_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->max_potential_tess_factor = src->max_potential_tess_factor;
      break;
    }
    case SM50_SHADER_ROOT_SIGNATURE:
    case SM50_SHADER_ROOT_SIGNATURE2: {
      struct SM50_SHADER_ROOT_SIGNATURE_DATA32 *src = (void *)args32;
      struct SM50_SHADER_ROOT_SIGNATURE_DATA *data = malloc(sizeof(struct SM50_SHADER_ROOT_SIGNATURE_DATA));
      last_arg->next = data;
      last_arg = (void *)data;
      last_arg->next = NULL;
      data->type = src->type;
      data->bytecode = UInt32ToPtr(src->bytecode);
      data->bytecode_length = src->bytecode_length;
      break;
    }
    case SM50_SHADER_ARGUMENT_TYPE_MAX:
      break;
    }
    args32 = UInt32ToPtr(args32->next);
  }
}

void
sm50_compilation_argument32_free(struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *first_arg) {
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *arg = first_arg->next;

  while (arg) {
    struct SM50_SHADER_COMPILATION_ARGUMENT_DATA *next = arg->next;
    free(arg);
    arg = next;
  }
}

NTSTATUS
thunk32_SM50Compile(void *args) {
  struct sm50_compile_params32 *params = args;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA first_arg;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 *args32 = UInt32ToPtr(params->args);
  sm50_compilation_argument32_convert(&first_arg, args32);

  params->ret = SM50Compile(
      params->shader, &first_arg, UInt32ToPtr(params->func_name), UInt32ToPtr(params->bitcode),
      UInt32ToPtr(params->error)
  );

  sm50_compilation_argument32_free(&first_arg);

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50GetCompiledBitcode(void *args) {
  struct sm50_get_compiled_bitcode_params32 *params = args;

  SM50GetCompiledBitcode(params->bitcode, UInt32ToPtr(params->data_out));

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50GetErrorMessage(void *args) {
  struct sm50_get_error_message_params32 *params = args;

  params->ret_size = SM50GetErrorMessage(params->error, UInt32ToPtr(params->buffer), params->buffer_size);

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50CompileTessellationPipelineHull(void *args) {
  struct sm50_compile_tessellation_pipeline_hull_params32 *params = args;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA first_arg;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 *args32 = UInt32ToPtr(params->hull_args);
  sm50_compilation_argument32_convert(&first_arg, args32);

  params->ret = SM50CompileTessellationPipelineHull(
      params->vertex, params->hull, &first_arg, UInt32ToPtr(params->func_name), UInt32ToPtr(params->bitcode),
      UInt32ToPtr(params->error)
  );

  sm50_compilation_argument32_free(&first_arg);

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50CompileTessellationPipelineDomain(void *args) {
  struct sm50_compile_tessellation_pipeline_domain_params32 *params = args;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA first_arg;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 *args32 = UInt32ToPtr(params->domain_args);
  sm50_compilation_argument32_convert(&first_arg, args32);

  params->ret = SM50CompileTessellationPipelineDomain(
      params->hull, params->domain, &first_arg, UInt32ToPtr(params->func_name), UInt32ToPtr(params->bitcode),
      UInt32ToPtr(params->error)
  );

  sm50_compilation_argument32_free(&first_arg);

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50CompileGeometryPipelineVertex(void *args) {
  struct sm50_compile_geometry_pipeline_vertex_params32 *params = args;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA first_arg;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 *args32 = UInt32ToPtr(params->vertex_args);
  sm50_compilation_argument32_convert(&first_arg, args32);

  params->ret = SM50CompileGeometryPipelineVertex(
      params->vertex, params->geometry, &first_arg, UInt32ToPtr(params->func_name), UInt32ToPtr(params->bitcode),
      UInt32ToPtr(params->error)
  );

  sm50_compilation_argument32_free(&first_arg);

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50CompileGeometryPipelineGeometry(void *args) {
  struct sm50_compile_geometry_pipeline_geometry_params32 *params = args;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA first_arg;
  struct SM50_SHADER_COMPILATION_ARGUMENT_DATA32 *args32 = UInt32ToPtr(params->geometry_args);
  sm50_compilation_argument32_convert(&first_arg, args32);

  params->ret = SM50CompileGeometryPipelineGeometry(
      params->vertex, params->geometry, &first_arg, UInt32ToPtr(params->func_name), UInt32ToPtr(params->bitcode),
      UInt32ToPtr(params->error)
  );

  sm50_compilation_argument32_free(&first_arg);

  return STATUS_SUCCESS;
}

NTSTATUS
thunk32_SM50GetArgumentsInfo(void *args) {
  struct sm50_get_arguments_info_params32 *params = args;

  SM50GetArgumentsInfo(params->shader, UInt32ToPtr(params->constant_buffers), UInt32ToPtr(params->arguments));

  return STATUS_SUCCESS;
}
#endif /* DXMT_NATIVE */
