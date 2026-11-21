/*
 * Copyright 2026 Feifan He for CodeWeavers
 * Copyright 2026 Paweł Kołodziejski
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2.1 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin St, Fifth Floor, Boston, MA 02110-1301, USA
 */

#include <metal_common>
#include <metal_texture>
#include <metal_math>
#include <metal_integer>
#include <metal_matrix>
#include <metal_geometric>
#include "dxmt_command_constants.hpp"

using namespace metal;
using namespace dxmt;

[[kernel]] void clear_texture_1d_uint(
    texture1d<uint, access::read_write> tex [[texture(0)]],
    constant uint4& value [[buffer(1)]],
    ushort pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  if(width > pos) {
    tex.write(value, pos);
  }
}

[[kernel]] void clear_texture_1d_array_uint(
    texture1d_array<uint, access::read_write> tex [[texture(0)]],
    constant uint4& value [[buffer(1)]],
    ushort2 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint array_len = tex.get_array_size();
  if(width > pos.x && array_len > pos.y) {
    tex.write(value, pos.x, pos.y);
  }
}

[[kernel]] void clear_texture_2d_uint(
    texture2d<uint, access::read_write> tex [[texture(0)]],
    constant uint4& value [[buffer(1)]],
    ushort2 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint height = tex.get_height();
  if(width > pos.x && height > pos.y) {
    tex.write(value, pos);
  }
}

[[kernel]] void clear_texture_2d_array_uint(
    texture2d_array<uint, access::read_write> tex [[texture(0)]],
    constant uint4& value [[buffer(1)]],
    ushort3 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint height = tex.get_height();
  uint array_len = tex.get_array_size();
  if(width > pos.x && height > pos.y && array_len > pos.z) {
    tex.write(value, pos.xy, pos.z);
  }
}

[[kernel]] void clear_texture_3d_uint(
    texture3d<uint, access::read_write> tex [[texture(0)]],
    constant uint4& value [[buffer(1)]],
    ushort3 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint height = tex.get_height();
  uint depth = tex.get_depth();
  if(width > pos.x && height > pos.y && depth > pos.z) {
    tex.write(value, pos);
  }
}

struct DXMTClearTextureBufferUInt {
  uint4 value;
  uint offset;
  uint size;
};

[[kernel]] void clear_texture_buffer_uint(
    texture_buffer<uint, access::read_write> tex [[texture(0)]],
    constant DXMTClearTextureBufferUInt& args [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint offset = pos + args.offset;
  if (args.size > pos && width > offset) {
    tex.write(args.value, offset);
  }
}

[[kernel]] void clear_buffer_uint(
    device uint* tex [[buffer(0)]],
    constant uint& size [[buffer(2)]],
    constant uint4& value [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  if(pos < size) {
    tex[pos] = value.x;
  }
}

[[kernel]] void clear_texture_1d_float(
    texture1d<float, access::read_write> tex [[texture(0)]],
    constant float4& value [[buffer(1)]],
    ushort pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  if(width > pos) {
    tex.write(value, pos);
  }
}

[[kernel]] void clear_texture_1d_array_float(
    texture1d_array<float, access::read_write> tex [[texture(0)]],
    constant float4& value [[buffer(1)]],
    ushort2 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint array_len = tex.get_array_size();
  if(width > pos.x && array_len > pos.y) {
    tex.write(value, pos.x, pos.y);
  }
}

[[kernel]] void clear_texture_2d_float(
    texture2d<float, access::read_write> tex [[texture(0)]],
    constant float4& value [[buffer(1)]],
    ushort2 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint height = tex.get_height();
  if(width > pos.x && height > pos.y) {
    tex.write(value, pos);
  }
}

[[kernel]] void clear_texture_2d_array_float(
    texture2d_array<float, access::read_write> tex [[texture(0)]],
    constant float4& value [[buffer(1)]],
    ushort3 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint height = tex.get_height();
  uint array_len = tex.get_array_size();
  if(width > pos.x && height > pos.y && array_len > pos.z) {
    tex.write(value, pos.xy, pos.z);
  }
}

[[kernel]] void clear_texture_3d_float(
    texture3d<float, access::read_write> tex [[texture(0)]],
    constant float4& value [[buffer(1)]],
    ushort3 pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint height = tex.get_height();
  uint depth = tex.get_depth();
  if(width > pos.x && height > pos.y && depth > pos.z) {
    tex.write(value, pos);
  }
}

struct DXMTClearTextureBufferFloat {
  float4 value;
  uint offset;
  uint size;
};

[[kernel]] void clear_texture_buffer_float(
    texture_buffer<float, access::read_write> tex [[texture(0)]],
    constant DXMTClearTextureBufferFloat& args [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  uint width = tex.get_width();
  uint offset = pos + args.offset;
  if (args.size > pos && width > offset) {
    tex.write(args.value, offset);
  }
}

struct clear_data {
  float4 position [[position]];
  uint array_index [[render_target_array_index]];
};

[[vertex]] clear_data vs_clear_rt(
  ushort id [[vertex_id]],
  ushort instance_id [[instance_id]]
) {
  clear_data output;
  float2 uv = float2((id << 1) & 2, id & 2);
  output.position = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
  output.array_index = instance_id;
  return output;
}

[[fragment]] float4 fs_clear_rt_float(
  constant float4& clear_value [[buffer(kCustomBufferArgumentIndex0)]]
) {
  return clear_value;
}

[[fragment]] uint4 fs_clear_rt_uint(
  constant float4& clear_value [[buffer(kCustomBufferArgumentIndex0)]]
) {
  return (uint4)clear_value;
}

[[fragment]] int4 fs_clear_rt_sint(
  constant float4& clear_value [[buffer(kCustomBufferArgumentIndex0)]]
) {
  return (int4)clear_value;
}

struct depth_out {
  float depth [[depth(any)]];
};

[[fragment]] depth_out fs_clear_rt_depth (
  constant float4& clear_value [[buffer(kCustomBufferArgumentIndex0)]]
) {
  return {clear_value.x};
}

struct present_data {
  float4 position [[position]];
  float2 uv [[user(coord)]];
};

[[vertex]] present_data vs_present_quad(ushort id [[vertex_id]]) {
  present_data output;
  float2 uv = float2((id << 1) & 2, id & 2);
	output.position = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
  output.uv = uv;
  return output;
}

constant constexpr float PQ_M1 = 0.1593017578125;
constant constexpr float PQ_M2 = 78.84375;
constant constexpr float PQ_C1 = 0.8359375;
constant constexpr float PQ_C2 = 18.8515625;
constant constexpr float PQ_C3 = 18.6875;

float linear_to_pq(float linear) {
  linear = linear / 10000;
  return pow((PQ_C1 + PQ_C2 * pow(abs(linear), PQ_M1)) / (1.0f + PQ_C3 * pow(abs(linear), PQ_M1)), PQ_M2);
}

float3 linear_to_pq(float3 linear) {
  linear = linear / 10000;
  return pow((PQ_C1 + PQ_C2 * pow(abs(linear), PQ_M1)) / (1.0f + PQ_C3 * pow(abs(linear), PQ_M1)), PQ_M2);
}

float pq_to_linear(float norm) {
  return 10000 * pow(abs(max(pow(norm, 1.0 / PQ_M2) -PQ_C1, 0.0f) / (PQ_C2 - PQ_C3 * pow(abs(norm), 1.0f / PQ_M2))), 1.0f / PQ_M1 );
}

float3 pq_to_linear(float3 norm) {
  return 10000 * pow(abs(max(pow(norm, 1.0 / PQ_M2) -PQ_C1, 0.0f) / (PQ_C2 - PQ_C3 * pow(abs(norm), 1.0f / PQ_M2))), 1.0f / PQ_M1 );
}

// See BT.2408 Annex 5
float EETF(float E, float Lw, float Lb, float Lmax, float Lmin) {
  float pq_Lb = linear_to_pq(Lb);
  float pqdiff_LwLb = linear_to_pq(Lw) - pq_Lb;
  float min_lum = (linear_to_pq(Lmin) - pq_Lb) / pqdiff_LwLb;
  float max_lum = (linear_to_pq(Lmax) - pq_Lb) / pqdiff_LwLb;
  E = (E - pq_Lb) / pqdiff_LwLb;
  float KS = 1.5 * max_lum - 0.5;
  float b = min_lum;
  if (E >= KS) {
    float T = (E - KS) /  (1 - KS);
    float T_2 = T * T;
    float T_3 = T_2 * T;
    E = (2 * T_3 - 3 * T_2 + 1) * KS 
        + (T_3 - 2 * T_2 + T) * (1 - KS)
        + (- 2 * T_3 + 3 * T_2) * max_lum;
  }
  E = E + b * pow(1 - E, 4);
  return E * pqdiff_LwLb + pq_Lb;
}

constant bool present_backbuffer_size_matched [[function_constant(kPresentFCIndex_BackbufferSizeMatched)]];
constant bool present_backbuffer_is_srgb [[function_constant(kPresentFCIndex_BackbufferIsSRGB)]];
constant bool present_hdr_pq [[function_constant(kPresentFCIndex_HDRPQ)]];
constant bool present_with_hdr_metadata [[function_constant(kPresentFCIndex_WithHDRMetadata)]];
constant bool present_backbuffer_is_ms [[function_constant(kPresentFCIndex_BackbufferIsMS)]];
constant bool present_backbuffer_is_not_ms = !present_backbuffer_is_ms;
constant bool present_gamma_enabled [[function_constant(kPresentFCIndex_GammaEnabled)]];

constexpr sampler s(coord::normalized);
constexpr sampler gamma_sampler(coord::normalized, filter::linear, address::clamp_to_edge);

struct DXMTPresentMetadata {
  float edr_scale;
  float max_content_luminance;
  float max_display_luminance;
};

float3 to_srgb(float3 linear) {
  return select(1.055 * pow(linear, 1.0 / 2.4) - 0.055, linear * 12.92, linear < 0.0031308); 
}

[[fragment]] float4 fs_present_quad(
    present_data input [[stage_in]],
    texture2d<float, access::sample> source [[texture(0), function_constant(present_backbuffer_is_not_ms)]],
    texture2d_ms<float, access::read> source_ms [[texture(0), function_constant(present_backbuffer_is_ms)]],
    texture2d<float, access::sample> gamma_lut [[texture(1), function_constant(present_gamma_enabled)]],
    constant DXMTPresentMetadata& meta [[buffer(0)]]
) {
  float4 output = float4(0);
  if (present_backbuffer_is_ms) {
    const uint count = source_ms.get_num_samples();
    for (uint i = 0; i < count; i++) {
      output += source_ms.read(uint2(input.position.xy), i);
    }
    output /= count;
  } else {
    output = present_backbuffer_size_matched
      ? source.read(uint2(input.position.xy))
      : source.sample(s, input.uv);
  }
  float3 output_rgb = output.xyz;
  if (present_gamma_enabled && !present_hdr_pq && !present_with_hdr_metadata) {
    output_rgb = float3(
        gamma_lut.sample(gamma_sampler, float2(output_rgb.r, 0.5)).r,
        gamma_lut.sample(gamma_sampler, float2(output_rgb.g, 0.5)).g,
        gamma_lut.sample(gamma_sampler, float2(output_rgb.b, 0.5)).b);
  }
  float edr_scale = meta.edr_scale;
  if (present_backbuffer_is_srgb)
    output_rgb = to_srgb(output_rgb);
  if (present_hdr_pq)
    output_rgb = pq_to_linear(output_rgb);
  else if (present_with_hdr_metadata)
    output_rgb = output_rgb * 80; // to nits
  if (present_with_hdr_metadata) {
    // tone mapping: BT.2408 EETF + maxRGB
    float max_content_luminance = meta.max_content_luminance;
    float min_content_luminance = 1; // in case of division by 0
    float max_display_luminance = meta.max_display_luminance;
    float min_display_luminance = 0;
    float3 clamped = clamp(output_rgb, float3(min_content_luminance), float3(max_content_luminance));
    float3 clamped_pq = linear_to_pq(clamped);
    float maxRGB = max3(clamped_pq.x, clamped_pq.y, clamped_pq.z);
    float maxRGB2 = EETF(
      maxRGB, max_content_luminance, min_content_luminance, max_display_luminance, min_display_luminance
    );
    output_rgb = pq_to_linear(maxRGB2) / pq_to_linear(maxRGB) * clamped;
  }
  output_rgb *= edr_scale;
  if (present_hdr_pq)
    output_rgb = linear_to_pq(output_rgb);
  else if (present_with_hdr_metadata)
    output_rgb = output_rgb / 80;
  return float4(output_rgb, output.w);
}

struct D3D9HdrUniforms {
  float l_hdr_nits;
  float p_hdr;
  float log2_p_hdr;
  float inv_p_minus_one;
};

constant float L_SDR = 100.0f;
constant float P_SDR = 5.4395284707f;
constant float LOG_P_SDR = 1.6937747f;

inline float3 apply_gamma_ramp(float3 encoded, constant ushort4* lut) {
  float3 x = clamp(encoded, 0.0f, 1.0f) * 255.0f;
  float3 lo = floor(x);
  float3 frac = x - lo;
  uint3 i0 = uint3(lo);
  uint3 i1 = min(i0 + 1u, uint3(255u));
  float3 a = float3(lut[i0.x].r, lut[i0.y].g, lut[i0.z].b);
  float3 b = float3(lut[i1.x].r, lut[i1.y].g, lut[i1.z].b);
  return mix(a, b, frac) * (1.0f / 65535.0f);
}

inline float3 srgb_eotf(float3 encoded) {
  return select(
      pow((encoded + 0.055f) / 1.055f, float3(2.4f)),
      encoded / 12.92f,
      encoded <= float3(0.04045f)
  );
}

constant float3x3 M_BT709_TO_LMS = float3x3(
    float3(0.2958197875977f, 0.1562652587891f, 0.0351760864258f),
    float3(0.6230921020508f, 0.7272338867188f, 0.1564789062500f),
    float3(0.0810847778320f, 0.1164960937500f, 0.8083459472656f)
);

constant float3x3 M_LMS_TO_BT709 = float3x3(
    float3( 6.1729507446289f, -1.3236293334961f, -0.0118084289551f),
    float3(-5.3198394775391f,  2.5602416992188f, -0.2641143798828f),
    float3( 0.1465606689453f, -0.2363464355469f,  1.2761764526367f)
);

constant float3x3 M_LMS_PQ_TO_ICTCP = float3x3(
    float3(0.5f,  1.61370003f,  4.37806224f),
    float3(0.5f, -3.32339620f, -4.24553966f),
    float3(0.0f,  1.70969617f, -0.13252264f)
);

constant float3x3 M_ICTCP_TO_LMS_PQ = float3x3(
    float3(1.0f, 1.0f, 1.0f),
    float3(0.00860903f, -0.00860903f,  0.56003270f),
    float3(0.11102963f, -0.11102963f, -0.32062717f)
);

inline float3 pq_oetf(float3 nits) {
  constexpr float M1 = 2610.0f / 16384.0f;
  constexpr float M2 = 2523.0f / 4096.0f * 128.0f;
  constexpr float C1 = 3424.0f / 4096.0f;
  constexpr float C2 = 2413.0f / 4096.0f * 32.0f;
  constexpr float C3 = 2392.0f / 4096.0f * 32.0f;
  float3 y = pow(max(nits, float3(0.0f)) / 10000.0f, float3(M1));
  return pow((float3(C1) + C2 * y) / (float3(1.0f) + C3 * y), float3(M2));
}

inline float3 pq_eotf(float3 pq) {
  constexpr float M1 = 2610.0f / 16384.0f;
  constexpr float M2 = 2523.0f / 4096.0f * 128.0f;
  constexpr float C1 = 3424.0f / 4096.0f;
  constexpr float C2 = 2413.0f / 4096.0f * 32.0f;
  constexpr float C3 = 2392.0f / 4096.0f * 32.0f;
  float3 e = pow(max(pq, float3(0.0f)), float3(1.0f / M2));
  float3 num = max(e - float3(C1), float3(0.0f));
  float3 den = max(float3(C2) - C3 * e, float3(1e-20f));
  return 10000.0f * pow(num / den, float3(1.0f / M1));
}

inline float3 bt2446_ictcp(float3 lin, constant D3D9HdrUniforms& u) {
  float y_sdr = max(dot(lin, float3(0.2126f, 0.7152f, 0.0722f)), 1e-20f);
  float yp_sdr = pow(y_sdr, 1.0f / 2.4f);
  float yp_c = log((yp_sdr * (P_SDR - 1.0f)) + 1.0f) / LOG_P_SDR;
  float yp_0 = yp_c / 1.0770f;
  float yp_1 = (-2.7811f + sqrt(4.83307641f - 4.604f * yp_c)) / -2.302f;
  float yp_2 = (yp_c - 0.5f) / 0.5f;
  float yp_p = yp_0 <= 0.7399f ? yp_0
             : (yp_2 >= 0.9909f ? yp_2 : yp_1);
  float yp_hdr = (exp2(yp_p * u.log2_p_hdr) - 1.0f) * u.inv_p_minus_one;
  float y_hdr_linear_nits = pow(yp_hdr, 2.4f) * u.l_hdr_nits;

  float3 lms_nits = M_BT709_TO_LMS * (lin * L_SDR);
  float3 lms_pq = pq_oetf(lms_nits);
  float3 ictcp_in = M_LMS_PQ_TO_ICTCP * lms_pq;

  float i_hdr = pq_oetf(float3(y_hdr_linear_nits)).x;
  float i_ratio = i_hdr / max(ictcp_in.x, 1e-6f);
  float3 ictcp_out = float3(i_hdr, ictcp_in.y * i_ratio, ictcp_in.z * i_ratio);
  float3 lms_pq_out = M_ICTCP_TO_LMS_PQ * ictcp_out;
  float3 lms_nits_out = pq_eotf(lms_pq_out);
  float3 rgb_nits_out = M_LMS_TO_BT709 * lms_nits_out;

  return max(rgb_nits_out, float3(0.0f)) / L_SDR;
}

[[fragment]] float4 fs_present_d3d9_sdr(
  present_data input [[stage_in]],
  texture2d<float> source [[texture(0)]]
) {
  constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
  return source.sample(s, input.uv);
}

[[fragment]] float4 fs_present_d3d9_sdr_gamma(
  present_data input [[stage_in]],
  texture2d<float> source [[texture(0)]],
  constant ushort4* gamma_lut [[buffer(1)]]
) {
  constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
  float4 sample = source.sample(s, input.uv);
  return float4(apply_gamma_ramp(sample.rgb, gamma_lut), sample.a);
}

[[fragment]] float4 fs_present_d3d9_hdr_passthrough(
  present_data input [[stage_in]],
  texture2d<float> source [[texture(0)]]
) {
  constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
  float4 sample = source.sample(s, input.uv);
  return float4(srgb_eotf(sample.rgb), sample.a);
}

[[fragment]] float4 fs_present_d3d9_hdr_passthrough_gamma(
  present_data input [[stage_in]],
  texture2d<float> source [[texture(0)]],
  constant ushort4* gamma_lut [[buffer(1)]]
) {
  constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
  float4 sample = source.sample(s, input.uv);
  return float4(srgb_eotf(apply_gamma_ramp(sample.rgb, gamma_lut)), sample.a);
}

[[fragment]] float4 fs_present_d3d9_hdr_bt2446(
  present_data input [[stage_in]],
  texture2d<float> source [[texture(0)]],
  constant D3D9HdrUniforms& u [[buffer(0)]]
) {
  constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
  float4 sample = source.sample(s, input.uv);
  return float4(bt2446_ictcp(srgb_eotf(sample.rgb), u), sample.a);
}

[[fragment]] float4 fs_present_d3d9_hdr_bt2446_gamma(
  present_data input [[stage_in]],
  texture2d<float> source [[texture(0)]],
  constant D3D9HdrUniforms& u [[buffer(0)]],
  constant ushort4* gamma_lut [[buffer(1)]]
) {
  constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
  float4 sample = source.sample(s, input.uv);
  float3 ramped = apply_gamma_ramp(sample.rgb, gamma_lut);
  return float4(bt2446_ictcp(srgb_eotf(ramped), u), sample.a);
}

struct DXMTDispatchArguments {
  uint x;
  uint y;
  uint z;
};

struct DXMTTSDispatchMarshal {
  constant uint2& draw_arguments; // (vertex|index_count, instance_count)
  device DXMTDispatchArguments& dispatch_arguments_out;
  ulong max_object_threadgroups;
  ushort control_point_count;
  ushort patch_per_group;
  uint end_of_command;
};

[[vertex]] void ts_draw_arguments_marshal(
    constant DXMTTSDispatchMarshal* tasks [[buffer(kCustomBufferArgumentIndex0)]]
) {
  uint index = 0;
  for(;;) {
    constant DXMTTSDispatchMarshal& task = tasks[index];

    device DXMTDispatchArguments& output = task.dispatch_arguments_out;

    uint patch_count_per_instance = task.draw_arguments.x / (uint)task.control_point_count;
    uint x = (patch_count_per_instance - 1) / task.patch_per_group + 1;
    if ((task.draw_arguments.x * task.draw_arguments.y) == 0 ||
        x * task.draw_arguments.y > task.max_object_threadgroups) {
      output.x = 0;
      output.y = 0;
      output.z = 0;
    } else {
      output.x = x;
      output.y = task.draw_arguments.y;
      output.z = 1;
    }

    if (task.end_of_command)
      break;
    index++;
  };
}

struct DXMTGSDispatchMarshal {
  constant uint2& draw_arguments; // (vertex|index_count, instance_count)
  device DXMTDispatchArguments& dispatch_arguments_out;
  ulong max_object_threadgroups;
  uint vertex_count_per_warp;
  uint end_of_command;
};

[[vertex]] void gs_draw_arguments_marshal(
    constant DXMTGSDispatchMarshal* tasks [[buffer(kCustomBufferArgumentIndex0)]]
) {
  uint index = 0;
  for(;;) {
    constant DXMTGSDispatchMarshal& task = tasks[index];

    device DXMTDispatchArguments& output = task.dispatch_arguments_out;

    uint x = (task.draw_arguments.x - 1) / task.vertex_count_per_warp + 1;
    if ((task.draw_arguments.x * task.draw_arguments.y) == 0 ||
        x * task.draw_arguments.y > task.max_object_threadgroups) {
      output.x = 0;
      output.y = 0;
      output.z = 0;
    } else {
      output.x = x;
      output.y = task.draw_arguments.y;
      output.z = 1;
    }

    if (task.end_of_command)
      break;
    index++;
  };
}

struct depth_stencil_out {
  float depth [[depth(any)]];
  uint stencil [[stencil]]; 
};

struct packed_d32s8x24 {
  float depth;
  uchar stencil;
  uchar unused[3];
};

struct linear_texture_desc {
  uint bytes_per_row;
  uint bytes_per_image;
};

[[fragment]] depth_stencil_out fs_copy_from_buffer_d32s8(
  present_data input [[stage_in]],
  device char* buffer [[buffer(kCustomBufferArgumentIndex0)]],
  constant linear_texture_desc& desc [[buffer(kCustomBufferArgumentIndex1)]]
) {
  depth_stencil_out result;
  uint2 pos = uint2(input.position.xy);
  uint buffer_offset = pos.x * sizeof(packed_d32s8x24) 
                        + pos.y * desc.bytes_per_row;
  packed_d32s8x24 data = *reinterpret_cast<device packed_d32s8x24 *>(buffer + buffer_offset);
  result.depth = data.depth;
  result.stencil = data.stencil;
  return result;
}

[[fragment]] depth_stencil_out fs_copy_from_buffer_d24s8(
  present_data input [[stage_in]],
  device char* buffer [[buffer(kCustomBufferArgumentIndex0)]],
  constant linear_texture_desc& desc [[buffer(kCustomBufferArgumentIndex1)]]
) {
  depth_stencil_out result;
  uint2 pos = uint2(input.position.xy);
  uint buffer_offset = pos.x * sizeof(uint) 
                        + pos.y * desc.bytes_per_row;
  uint data = *reinterpret_cast<device uint *>(buffer + buffer_offset);
  result.depth = float(data & 0xffffff) / float(0xffffff);
  result.stencil = data >> 24;
  return result;
}

[[kernel]] void cs_copy_to_buffer_d32s8(
  texture2d<float, access::read> tex_depth [[texture(0)]],
  texture2d<uint, access::read> tex_stencil [[texture(1)]],
  device char* buffer [[buffer(0)]],
  constant linear_texture_desc& desc [[buffer(1)]],
  ushort2 pos [[thread_position_in_grid]]
) {
  uint width = tex_depth.get_width();
  uint height = tex_depth.get_height();
  uint buffer_offset = pos.x * sizeof(packed_d32s8x24) 
                        + pos.y * desc.bytes_per_row;
  if (width > pos.x && height > pos.y) {
    auto dst = reinterpret_cast<device packed_d32s8x24 *>(buffer + buffer_offset);
    dst->depth = tex_depth.read(pos).x;
    dst->stencil = tex_stencil.read(pos).x;
  }
}

[[kernel]] void cs_copy_to_buffer_d24s8(
  texture2d<float, access::read> tex_depth [[texture(0)]],
  texture2d<uint, access::read> tex_stencil [[texture(1)]],
  device char* buffer [[buffer(0)]],
  constant linear_texture_desc& desc [[buffer(1)]],
  ushort2 pos [[thread_position_in_grid]]
) {
  uint width = tex_depth.get_width();
  uint height = tex_depth.get_height();
  uint buffer_offset = pos.x * sizeof(uint) 
                        + pos.y * desc.bytes_per_row;
  if (width > pos.x && height > pos.y) {
    auto dst = reinterpret_cast<device uint *>(buffer + buffer_offset);
    float depth = tex_depth.read(pos).x;
    uint stencil = tex_stencil.read(pos).x;
    *dst = (uint(0xffffff * clamp(depth, 0.0, 1.0)) & 0xffffff) | (stencil << 24);
  }
}

struct DXMTClearFloatMetadata {
  float4 value;
  uint2 offset;
  uint2 size;
};

struct DXMTClearUintMetadata {
  uint4 value;
  uint2 offset;
  uint2 size;
};

[[kernel]] void cs_clear_texture2d_float(
    texture2d<float, access::write> tex [[texture(0)]],
    constant DXMTClearFloatMetadata& meta [[buffer(1)]],
    uint2 pos [[thread_position_in_grid]]
) {
  tex.write(meta.value, meta.offset + pos);
}

[[kernel]] void cs_clear_texture2d_array_float(
    texture2d_array<float, access::write> tex [[texture(0)]],
    constant DXMTClearFloatMetadata& meta [[buffer(1)]],
    uint3 pos [[thread_position_in_grid]]
) {
  tex.write(meta.value, meta.offset + pos.xy, pos.z);
}

[[kernel]] void cs_clear_texture3d_float(
    texture3d<float, access::write> tex [[texture(0)]],
    constant DXMTClearFloatMetadata& meta [[buffer(1)]],
    uint2 pos [[thread_position_in_grid]]
) {
  auto depth = tex.get_depth();
  for (uint i = 0; i < depth; i++) {
    uint2 final_pos = meta.offset + pos;
    tex.write(meta.value, uint3(final_pos.x, final_pos.y, i));
  }
}

[[kernel]] void cs_clear_texture2d_uint(
    texture2d<uint, access::write> tex [[texture(0)]],
    constant DXMTClearUintMetadata& meta [[buffer(1)]],
    uint2 pos [[thread_position_in_grid]]
) {
  tex.write(meta.value, meta.offset + pos);
}

[[kernel]] void cs_clear_texture2d_array_uint(
    texture2d_array<uint, access::write> tex [[texture(0)]],
    constant DXMTClearUintMetadata& meta [[buffer(1)]],
    uint3 pos [[thread_position_in_grid]]
) {
  tex.write(meta.value, meta.offset + pos.xy, pos.z);
}

[[kernel]] void cs_clear_texture3d_uint(
    texture3d<uint, access::write> tex [[texture(0)]],
    constant DXMTClearUintMetadata& meta [[buffer(1)]],
    uint2 pos [[thread_position_in_grid]]
) {
  auto depth = tex.get_depth();
  for (uint i = 0; i < depth; i++) {
    uint2 final_pos = meta.offset + pos;
    tex.write(meta.value, uint3(final_pos.x, final_pos.y, i));
  }
}

[[kernel]] void cs_clear_tbuffer_float(
    texture_buffer<float, access::write> tex [[texture(0)]],
    constant DXMTClearFloatMetadata& meta [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  tex.write(meta.value, meta.offset.x + pos);
}

[[kernel]] void cs_clear_tbuffer_uint(
    texture_buffer<uint, access::write> tex [[texture(0)]],
    constant DXMTClearUintMetadata& meta [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  tex.write(meta.value, meta.offset.x + pos);
}

[[kernel]] void cs_clear_buffer_float(
    device float* buffer [[buffer(0)]],
    constant DXMTClearFloatMetadata& meta [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  buffer[meta.offset.x + pos] = meta.value.x;
}

[[kernel]] void cs_clear_buffer_uint(
    device uint* buffer [[buffer(0)]],
    constant DXMTClearUintMetadata& meta [[buffer(1)]],
    uint pos [[thread_position_in_grid]]
) {
  buffer[meta.offset.x + pos] = meta.value.x;
}

[[kernel]] void cs_downscale_dilated_mv(
  uint2 pos [[thread_position_in_grid]],
  constant float2& mv_scale [[buffer(0)]],
  texture2d<float, access::read> dilated [[texture(0)]],
  texture2d<float, access::write> downscaled [[texture(1)]]
) {
  float2 source_size = float2(dilated.get_width(), dilated.get_height());
  float2 viewport_size = float2(downscaled.get_width(), downscaled.get_height());
  float2 scale = viewport_size / source_size;
  float2 sample_uv = (float2(pos) + 0.5) / viewport_size;
  uint2 sample_pos = uint2(sample_uv * source_size);
  float2 hi_mv = dilated.read(sample_pos).xy;
  float2 hi_mv_pixel = hi_mv * mv_scale;
  float2 lo_mv_pixel = hi_mv_pixel * scale;
  downscaled.write(lo_mv_pixel.xyxy, pos); 
}

[[kernel]] void tile_barrier(ushort2 pos [[thread_position_in_threadgroup]]) {
  // empty
}
