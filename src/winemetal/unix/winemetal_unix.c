#include <stdatomic.h>
#include <dlfcn.h>
#import <Cocoa/Cocoa.h>
#import <ColorSync/ColorSync.h>
#import <CoreFoundation/CFRunLoop.h>
#import <Metal/Metal.h>
#import <MetalFX/MetalFX.h>
#import <QuartzCore/QuartzCore.h>
#include "objc/objc-runtime.h"
#include <bootstrap.h>
#include <mach/mach_port.h>
#define WINEMETAL_API
#include "../winemetal_thunks.h"
#include "../airconv_thunks.h"

typedef int NTSTATUS;
#define STATUS_SUCCESS 0
#define STATUS_UNSUCCESSFUL 0xC0000001
#define STATUS_NOT_IMPLEMENTED 0xC0000002


NTSTATUS _NSObject_retain(void *obj);

NTSTATUS _NSObject_release(void *obj);

NTSTATUS _NSArray_object(void *obj);

NTSTATUS _NSArray_count(void *obj);

NTSTATUS _MTLCopyAllDevices(void *obj);

NTSTATUS _MTLDevice_recommendedMaxWorkingSetSize(void *obj);

NTSTATUS _MTLDevice_currentAllocatedSize(void *obj);

NTSTATUS _MTLDevice_name(void *obj);

NTSTATUS _NSString_getCString(void *obj);

NTSTATUS _MTLDevice_newCommandQueue(void *obj);

NTSTATUS _NSAutoreleasePool_alloc_init(void *obj);

NTSTATUS _MTLCommandQueue_commandBuffer(void *obj);

NTSTATUS _MTLCommandBuffer_commit(void *obj);

NTSTATUS _MTLCommandBuffer_waitUntilCompleted(void *obj);

NTSTATUS _MTLCommandBuffer_status(void *obj);

NTSTATUS _MTLDevice_newSharedEvent(void *obj);

NTSTATUS _MTLSharedEvent_signaledValue(void *obj);

NTSTATUS _MTLCommandBuffer_encodeSignalEvent(void *obj);

NTSTATUS _MTLDevice_newBuffer(void *obj);

NTSTATUS _MTLDevice_newSamplerState(void *obj);

NTSTATUS _MTLDevice_newDepthStencilState(void *obj);

/* The Rust half's (format.rs, texture.rs). */
MTLPixelFormat to_metal_pixel_format(enum WMTPixelFormat);
void fill_texture_descriptor(MTLTextureDescriptor *, struct WMTTextureInfo *);


NTSTATUS _MTLDevice_newTexture(void *obj);

NTSTATUS _MTLBuffer_newTexture(void *obj);

NTSTATUS _MTLTexture_newTextureView(void *obj);

NTSTATUS _MTLDevice_minimumLinearTextureAlignmentForPixelFormat(void *obj);

NTSTATUS _MTLDevice_newLibrary(void *obj);

NTSTATUS _MTLLibrary_newFunction(void *obj);

NTSTATUS _NSString_lengthOfBytesUsingEncoding(void *obj);

NTSTATUS _NSObject_description(void *obj);

NTSTATUS _MTLDevice_newComputePipelineState(void *obj);

NTSTATUS _MTLCommandBuffer_blitCommandEncoder(void *obj);

NTSTATUS _MTLCommandBuffer_computeCommandEncoder(void *obj);

NTSTATUS _MTLCommandBuffer_renderCommandEncoder(void *obj);

NTSTATUS _MTLCommandEncoder_endEncoding(void *obj);

#ifndef DXMT_NO_PRIVATE_API

#endif

NTSTATUS _MTLDevice_newRenderPipelineState(void *obj);

/* MacNeutron: slot 34's pipeline with a vertex descriptor (Direct3D 9 on winemetal). */
NTSTATUS _MTLDevice_newRenderPipelineStateWithVertexDescriptor(void *obj);

NTSTATUS _MTLDevice_newMeshRenderPipelineState(void *obj);

NTSTATUS _MTLBlitCommandEncoder_encodeCommands(void *obj);

NTSTATUS _MTLComputeCommandEncoder_encodeCommands(void *obj);

NTSTATUS _MTLRenderCommandEncoder_encodeCommands(void *obj);

NTSTATUS _MTLTexture_pixelFormat(void *obj);

NTSTATUS _MTLTexture_width(void *obj);

NTSTATUS _MTLTexture_height(void *obj);

NTSTATUS _MTLTexture_depth(void *obj);

NTSTATUS _MTLTexture_arrayLength(void *obj);

NTSTATUS _MTLTexture_mipmapLevelCount(void *obj);

NTSTATUS _MTLTexture_replaceRegion(void *obj);

NTSTATUS _MTLBuffer_didModifyRange(void *obj);

NTSTATUS _MTLCommandBuffer_presentDrawable(void *obj);

NTSTATUS _MTLCommandBuffer_presentDrawableAfterMinimumDuration(void *obj);

NTSTATUS _MTLDevice_supportsFamily(void *obj);

NTSTATUS _MTLDevice_supportsBCTextureCompression(void *obj);

NTSTATUS _MTLDevice_supportsTextureSampleCount(void *obj);

NTSTATUS _MTLDevice_hasUnifiedMemory(void *obj);

NTSTATUS _MTLCaptureManager_sharedCaptureManager(void *obj);

NTSTATUS _MTLCaptureManager_startCapture(void *obj);

NTSTATUS _MTLCaptureManager_stopCapture(void *obj);

NTSTATUS _MTLDevice_newTemporalScaler(void *obj);

NTSTATUS _MTLDevice_newSpatialScaler(void *obj);

NTSTATUS _MTLCommandBuffer_encodeTemporalScale(void *obj);

NTSTATUS _MTLCommandBuffer_encodeSpatialScale(void *obj);

NTSTATUS _NSString_string(void *obj);

NTSTATUS _NSString_alloc_init(void *obj);


NTSTATUS _DeveloperHUDProperties_instance(void *obj);

NTSTATUS _DeveloperHUDProperties_addLabel(void *obj);

NTSTATUS _DeveloperHUDProperties_updateLabel(void *obj);

NTSTATUS _DeveloperHUDProperties_remove(void *obj);

NTSTATUS _MetalDrawable_texture(void *obj);

NTSTATUS _MetalLayer_nextDrawable(void *obj);

NTSTATUS _MTLDevice_supportsFXSpatialScaler(void *obj);

NTSTATUS _MTLDevice_supportsFXTemporalScaler(void *obj);

NTSTATUS _MetalLayer_setProps(void *obj);

NTSTATUS _MetalLayer_getProps(void *obj);

NTSTATUS _CreateMetalViewFromHWND(void *obj);

NTSTATUS _ReleaseMetalView(void *obj);

NTSTATUS thunk_SM50Initialize(void *obj);

NTSTATUS thunk_SM50Destroy(void *obj);

NTSTATUS thunk_SM50Compile(void *obj);

NTSTATUS thunk_SM50GetCompiledBitcode(void *obj);

NTSTATUS thunk_SM50DestroyBitcode(void *obj);

NTSTATUS thunk_SM50GetErrorMessage(void *obj);

NTSTATUS thunk_SM50FreeError(void *obj);

NTSTATUS thunk_SM50CompileTessellationPipelineHull(void *obj);

NTSTATUS thunk_SM50CompileTessellationPipelineDomain(void *obj);

NTSTATUS thunk_SM50CompileGeometryPipelineVertex(void *obj);

NTSTATUS thunk_SM50CompileGeometryPipelineGeometry(void *obj);

NTSTATUS _MTLCommandEncoder_setLabel(void *obj);

NTSTATUS _MTLDevice_setShouldMaximizeConcurrentCompilation(void *obj);

NTSTATUS thunk_SM50GetArgumentsInfo(void *obj);

#ifndef DXMT_NATIVE

NTSTATUS thunk32_SM50Initialize(void *obj);

NTSTATUS thunk32_SM50Compile(void *obj);

NTSTATUS thunk32_SM50GetCompiledBitcode(void *obj);

NTSTATUS thunk32_SM50GetErrorMessage(void *obj);

NTSTATUS thunk32_SM50CompileTessellationPipelineHull(void *obj);

NTSTATUS thunk32_SM50CompileTessellationPipelineDomain(void *obj);

NTSTATUS thunk32_SM50CompileGeometryPipelineVertex(void *obj);

NTSTATUS thunk32_SM50CompileGeometryPipelineGeometry(void *obj);

NTSTATUS thunk32_SM50GetArgumentsInfo(void *obj);
#endif /* DXMT_NATIVE */

NTSTATUS _MTLCommandBuffer_error(void *obj);

NTSTATUS _MTLCommandBuffer_logs(void *obj);

NTSTATUS _MTLLogContainer_enumerate(void *obj);


NTSTATUS _CGColorSpace_checkColorSpaceSupported(void *obj);

NTSTATUS _MetalLayer_setColorSpace(void *obj);

NTSTATUS _WMTGetPrimaryDisplayId(void *obj);

NTSTATUS _WMTGetSecondaryDisplayId(void *obj);

NTSTATUS _WMTGetDisplayDescription(void *obj);

NTSTATUS _MetalLayer_getEDRValue(void *obj);

NTSTATUS _MTLLibrary_newFunctionWithConstants(void *obj);

NTSTATUS _WMTQueryDisplaySetting(void *obj);

NTSTATUS _WMTUpdateDisplaySetting(void *obj);

NTSTATUS _WMTQueryDisplaySettingForLayer(void *obj);

NTSTATUS _MTLCommandBuffer_encodeWaitForEvent(void *obj);

NTSTATUS _MTLSharedEvent_signalValue(void *obj);

NTSTATUS _MTLSharedEvent_setWin32EventAtValue(void *obj);

NTSTATUS _MTLSharedEvent_setWin32EventAtValues(void *obj);

NTSTATUS _SharedEventListener_start(void *obj);

NTSTATUS _SharedEventListener_create(void *obj);

NTSTATUS _SharedEventListener_destroy(void *obj);


NTSTATUS _MTLDevice_newFence(void *obj);

NTSTATUS _MTLDevice_newEvent(void *obj);

NTSTATUS _MTLBuffer_updateContents(void *obj);

NTSTATUS _WMTGetOSVersion(void *obj);

NTSTATUS _MTLDevice_newBinaryArchive(void *obj);

NTSTATUS _MTLBinaryArchive_serialize(void *obj);

NTSTATUS _DispatchData_alloc_init(void *obj);

NTSTATUS _MTLDevice_newSharedTexture(void *obj);

NTSTATUS _WMTBootstrapRegister(void *obj);

NTSTATUS _WMTBootstrapLookUp(void *obj);

NTSTATUS _MTLSharedEvent_createMachPort(void *obj);

NTSTATUS _MTLDevice_newSharedEventWithMachPort(void *obj);

NTSTATUS _MTLDevice_registryID(void *obj);

NTSTATUS _MTLSharedEvent_waitUntilSignaledValue(void *obj);

NTSTATUS _MTLCounterSampleBuffer_newTimestampBuffer(void *obj);

NTSTATUS _MTLCounterSampleBuffer_resolveCounterRange(void *obj);

NTSTATUS _MTLCommandBuffer_computeCommandEncoderWithSampleBuffers(void *obj);

NTSTATUS _MTLDevice_sampleTimestamps(void *obj);

NTSTATUS _DispatchData_copyBytes(void *obj);

NTSTATUS _MTLDevice_temporalScalerScaleRange(void *obj);

NTSTATUS _MTLDevice_newFrameInterpolator(void *obj);

NTSTATUS _MTLCommandBuffer_encodeFrameInterpolate(void *obj);

NTSTATUS _MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(void *obj);

NTSTATUS _MTLCommandBuffer_property(void *obj);

NTSTATUS _MTLDevice_newTileRenderPipelineState(void *obj);

NTSTATUS _MTLDevice_newResidencySet(void *obj);

NTSTATUS _MTLResidencySet_addAllocations(void *obj);

NTSTATUS _MTLResidencySet_removeAllocations(void *obj);

NTSTATUS _MTLResidencySet_removeAllAllocations(void *obj);

NTSTATUS _MTLResidencySet_commit(void *obj);

NTSTATUS _MTLCommandQueue_addResidencySet(void *obj);

NTSTATUS _MTLDevice_newHeap(void *obj);

NTSTATUS _MTLDevice_heapBufferSizeAndAlign(void *obj);

NTSTATUS _MTLDevice_heapTextureSizeAndAlign(void *obj);

NTSTATUS _MTLHeap_newBuffer(void *obj);

NTSTATUS _MTLHeap_newTexture(void *obj);

NTSTATUS _MTLDevice_newIndirectCommandBuffer(void *obj);

NTSTATUS _MTLDevice_newLibraryWithSource(void *obj);

/* MacNeutron: slot 144 with compile options (Direct3D 9 on winemetal). */
NTSTATUS _MTLDevice_newLibraryWithSourceOptions(void *obj);

/* MacNeutron: the guest's own slots (153-154, its memory; 155, Direct3D forwarding) in this table's place. */
static NTSTATUS
_MacNeutron_guestOnly(void *obj) {
  return STATUS_NOT_IMPLEMENTED;
}

NTSTATUS _MTLTexture_getBytes(void *obj);

/*
 * Definition from cache.c
 */

NTSTATUS _CacheReader_alloc_init(void *obj);
NTSTATUS _CacheReader_get(void *obj);
NTSTATUS _CacheWriter_alloc_init(void *obj);
NTSTATUS _CacheWriter_set(void *obj);
NTSTATUS _WMTSetMetalShaderCachePath(void *obj);

const void *__wine_unix_call_funcs[] = {
    &_NSObject_retain,
    &_NSObject_release,
    &_NSArray_object,
    &_NSArray_count,
    &_MTLCopyAllDevices,
    &_MTLDevice_recommendedMaxWorkingSetSize,
    &_MTLDevice_currentAllocatedSize,
    &_MTLDevice_name,
    &_NSString_getCString,
    &_MTLDevice_newCommandQueue,
    &_NSAutoreleasePool_alloc_init,
    &_MTLCommandQueue_commandBuffer,
    &_MTLCommandBuffer_commit,
    &_MTLCommandBuffer_waitUntilCompleted,
    &_MTLCommandBuffer_status,
    &_MTLDevice_newSharedEvent,
    &_MTLSharedEvent_signaledValue,
    &_MTLCommandBuffer_encodeSignalEvent,
    &_MTLDevice_newBuffer,
    &_MTLDevice_newSamplerState,
    &_MTLDevice_newDepthStencilState,
    &_MTLDevice_newTexture,
    &_MTLBuffer_newTexture,
    &_MTLTexture_newTextureView,
    &_MTLDevice_minimumLinearTextureAlignmentForPixelFormat,
    &_MTLDevice_newLibrary,
    &_MTLLibrary_newFunction,
    &_NSString_lengthOfBytesUsingEncoding,
    &_NSObject_description,
    &_MTLDevice_newComputePipelineState,
    &_MTLCommandBuffer_blitCommandEncoder,
    &_MTLCommandBuffer_computeCommandEncoder,
    &_MTLCommandBuffer_renderCommandEncoder,
    &_MTLCommandEncoder_endEncoding,
    &_MTLDevice_newRenderPipelineState,
    &_MTLDevice_newMeshRenderPipelineState,
    &_MTLBlitCommandEncoder_encodeCommands,
    &_MTLComputeCommandEncoder_encodeCommands,
    &_MTLRenderCommandEncoder_encodeCommands,
    &_MTLTexture_pixelFormat,
    &_MTLTexture_width,
    &_MTLTexture_height,
    &_MTLTexture_depth,
    &_MTLTexture_arrayLength,
    &_MTLTexture_mipmapLevelCount,
    &_MTLTexture_replaceRegion,
    &_MTLBuffer_didModifyRange,
    &_MTLCommandBuffer_presentDrawable,
    &_MTLCommandBuffer_presentDrawableAfterMinimumDuration,
    &_MTLDevice_supportsFamily,
    &_MTLDevice_supportsBCTextureCompression,
    &_MTLDevice_supportsTextureSampleCount,
    &_MTLDevice_hasUnifiedMemory,
    &_MTLCaptureManager_sharedCaptureManager,
    &_MTLCaptureManager_startCapture,
    &_MTLCaptureManager_stopCapture,
    &_MTLDevice_newTemporalScaler,
    &_MTLDevice_newSpatialScaler,
    &_MTLCommandBuffer_encodeTemporalScale,
    &_MTLCommandBuffer_encodeSpatialScale,
    &_NSString_string,
    &_NSString_alloc_init,
    &_DeveloperHUDProperties_instance,
    &_DeveloperHUDProperties_addLabel,
    &_DeveloperHUDProperties_updateLabel,
    &_DeveloperHUDProperties_remove,
    &_MetalDrawable_texture,
    &_MetalLayer_nextDrawable,
    &_MTLDevice_supportsFXSpatialScaler,
    &_MTLDevice_supportsFXTemporalScaler,
    &_MetalLayer_setProps,
    &_MetalLayer_getProps,
    &_CreateMetalViewFromHWND,
    &_ReleaseMetalView,
    &thunk_SM50Initialize,
    &thunk_SM50Destroy,
    &thunk_SM50Compile,
    &thunk_SM50GetCompiledBitcode,
    &thunk_SM50DestroyBitcode,
    &thunk_SM50GetErrorMessage,
    &thunk_SM50FreeError,
    &thunk_SM50CompileGeometryPipelineVertex,
    &thunk_SM50CompileGeometryPipelineGeometry,
    NULL,
    &thunk_SM50CompileTessellationPipelineHull,
    &thunk_SM50CompileTessellationPipelineDomain,
    &_MTLCommandEncoder_setLabel,
    &_MTLDevice_setShouldMaximizeConcurrentCompilation,
    &thunk_SM50GetArgumentsInfo,
    &_MTLCommandBuffer_error,
    &_MTLCommandBuffer_logs,
    &_MTLLogContainer_enumerate,
    &_CGColorSpace_checkColorSpaceSupported,
    &_MetalLayer_setColorSpace,
    &_WMTGetPrimaryDisplayId,
    &_WMTGetSecondaryDisplayId,
    &_WMTGetDisplayDescription,
    &_MetalLayer_getEDRValue,
    &_MTLLibrary_newFunctionWithConstants,
    &_WMTQueryDisplaySetting,
    &_WMTUpdateDisplaySetting,
    &_WMTQueryDisplaySettingForLayer,
    &_MTLCommandBuffer_encodeWaitForEvent,
    &_MTLSharedEvent_signalValue,
    &_MTLSharedEvent_setWin32EventAtValue,
    &_MTLDevice_newFence,
    &_MTLDevice_newEvent,
    &_MTLBuffer_updateContents,
    &_SharedEventListener_create,
    &_SharedEventListener_start,
    &_SharedEventListener_destroy,
    &_WMTGetOSVersion,
    &_MTLDevice_newBinaryArchive,
    &_MTLBinaryArchive_serialize,
    &_DispatchData_alloc_init,
    &_CacheReader_alloc_init,
    &_CacheReader_get,
    &_CacheWriter_alloc_init,
    &_CacheWriter_set,
    &_WMTSetMetalShaderCachePath,
    &_MTLDevice_newSharedTexture,
    &_WMTBootstrapRegister,
    &_WMTBootstrapLookUp,
    &_MTLSharedEvent_createMachPort,
    &_MTLDevice_newSharedEventWithMachPort,
    &_MTLDevice_registryID,
    &_MTLSharedEvent_waitUntilSignaledValue,
    &_MTLCounterSampleBuffer_newTimestampBuffer,
    &_MTLCounterSampleBuffer_resolveCounterRange,
    &_MTLCommandBuffer_blitCommandEncoderWithSampleBuffers,
    &_MTLCommandBuffer_property,
    &_MTLDevice_newTileRenderPipelineState,
    &_MTLDevice_newResidencySet,
    &_MTLResidencySet_addAllocations,
    &_MTLResidencySet_removeAllocations,
    &_MTLResidencySet_removeAllAllocations,
    &_MTLResidencySet_commit,
    &_MTLCommandQueue_addResidencySet,
    &_MTLDevice_newHeap,
    &_MTLDevice_heapBufferSizeAndAlign,
    &_MTLDevice_heapTextureSizeAndAlign,
    &_MTLHeap_newBuffer,
    &_MTLHeap_newTexture,
    &_MTLDevice_newIndirectCommandBuffer,
    &_MTLDevice_newLibraryWithSource,
    &_MTLTexture_getBytes,
    &_MTLSharedEvent_setWin32EventAtValues,
    &_MTLCommandBuffer_computeCommandEncoderWithSampleBuffers,
    &_MTLDevice_sampleTimestamps,
    &_DispatchData_copyBytes,
    &_MTLDevice_temporalScalerScaleRange,
    &_MTLDevice_newFrameInterpolator,
    &_MTLCommandBuffer_encodeFrameInterpolate,
    &_MacNeutron_guestOnly,
    &_MacNeutron_guestOnly,
    &_MacNeutron_guestOnly,
    &_MTLDevice_newRenderPipelineStateWithVertexDescriptor,
    &_MTLDevice_newLibraryWithSourceOptions,
};

#ifndef DXMT_NATIVE
const void *__wine_unix_call_wow64_funcs[] = {
    &_NSObject_retain,
    &_NSObject_release,
    &_NSArray_object,
    &_NSArray_count,
    &_MTLCopyAllDevices,
    &_MTLDevice_recommendedMaxWorkingSetSize,
    &_MTLDevice_currentAllocatedSize,
    &_MTLDevice_name,
    &_NSString_getCString,
    &_MTLDevice_newCommandQueue,
    &_NSAutoreleasePool_alloc_init,
    &_MTLCommandQueue_commandBuffer,
    &_MTLCommandBuffer_commit,
    &_MTLCommandBuffer_waitUntilCompleted,
    &_MTLCommandBuffer_status,
    &_MTLDevice_newSharedEvent,
    &_MTLSharedEvent_signaledValue,
    &_MTLCommandBuffer_encodeSignalEvent,
    &_MTLDevice_newBuffer,
    &_MTLDevice_newSamplerState,
    &_MTLDevice_newDepthStencilState,
    &_MTLDevice_newTexture,
    &_MTLBuffer_newTexture,
    &_MTLTexture_newTextureView,
    &_MTLDevice_minimumLinearTextureAlignmentForPixelFormat,
    &_MTLDevice_newLibrary,
    &_MTLLibrary_newFunction,
    &_NSString_lengthOfBytesUsingEncoding,
    &_NSObject_description,
    &_MTLDevice_newComputePipelineState,
    &_MTLCommandBuffer_blitCommandEncoder,
    &_MTLCommandBuffer_computeCommandEncoder,
    &_MTLCommandBuffer_renderCommandEncoder,
    &_MTLCommandEncoder_endEncoding,
    &_MTLDevice_newRenderPipelineState,
    &_MTLDevice_newMeshRenderPipelineState,
    &_MTLBlitCommandEncoder_encodeCommands,
    &_MTLComputeCommandEncoder_encodeCommands,
    &_MTLRenderCommandEncoder_encodeCommands,
    &_MTLTexture_pixelFormat,
    &_MTLTexture_width,
    &_MTLTexture_height,
    &_MTLTexture_depth,
    &_MTLTexture_arrayLength,
    &_MTLTexture_mipmapLevelCount,
    &_MTLTexture_replaceRegion,
    &_MTLBuffer_didModifyRange,
    &_MTLCommandBuffer_presentDrawable,
    &_MTLCommandBuffer_presentDrawableAfterMinimumDuration,
    &_MTLDevice_supportsFamily,
    &_MTLDevice_supportsBCTextureCompression,
    &_MTLDevice_supportsTextureSampleCount,
    &_MTLDevice_hasUnifiedMemory,
    &_MTLCaptureManager_sharedCaptureManager,
    &_MTLCaptureManager_startCapture,
    &_MTLCaptureManager_stopCapture,
    &_MTLDevice_newTemporalScaler,
    &_MTLDevice_newSpatialScaler,
    &_MTLCommandBuffer_encodeTemporalScale,
    &_MTLCommandBuffer_encodeSpatialScale,
    &_NSString_string,
    &_NSString_alloc_init,
    &_DeveloperHUDProperties_instance,
    &_DeveloperHUDProperties_addLabel,
    &_DeveloperHUDProperties_updateLabel,
    &_DeveloperHUDProperties_remove,
    &_MetalDrawable_texture,
    &_MetalLayer_nextDrawable,
    &_MTLDevice_supportsFXSpatialScaler,
    &_MTLDevice_supportsFXTemporalScaler,
    &_MetalLayer_setProps,
    &_MetalLayer_getProps,
    &_CreateMetalViewFromHWND,
    &_ReleaseMetalView,
    &thunk32_SM50Initialize,
    &thunk_SM50Destroy,
    &thunk32_SM50Compile,
    &thunk32_SM50GetCompiledBitcode,
    &thunk_SM50DestroyBitcode,
    &thunk32_SM50GetErrorMessage,
    &thunk_SM50FreeError,
    &thunk32_SM50CompileGeometryPipelineVertex,
    &thunk32_SM50CompileGeometryPipelineGeometry,
    NULL,
    &thunk32_SM50CompileTessellationPipelineHull,
    &thunk32_SM50CompileTessellationPipelineDomain,
    &_MTLCommandEncoder_setLabel,
    &_MTLDevice_setShouldMaximizeConcurrentCompilation,
    &thunk32_SM50GetArgumentsInfo,
    &_MTLCommandBuffer_error,
    &_MTLCommandBuffer_logs,
    &_MTLLogContainer_enumerate,
    &_CGColorSpace_checkColorSpaceSupported,
    &_MetalLayer_setColorSpace,
    &_WMTGetPrimaryDisplayId,
    &_WMTGetSecondaryDisplayId,
    &_WMTGetDisplayDescription,
    &_MetalLayer_getEDRValue,
    &_MTLLibrary_newFunctionWithConstants,
    &_WMTQueryDisplaySetting,
    &_WMTUpdateDisplaySetting,
    &_WMTQueryDisplaySettingForLayer,
    &_MTLCommandBuffer_encodeWaitForEvent,
    &_MTLSharedEvent_signalValue,
    &_MTLSharedEvent_setWin32EventAtValue,
    &_MTLDevice_newFence,
    &_MTLDevice_newEvent,
    &_MTLBuffer_updateContents,
    &_SharedEventListener_create,
    &_SharedEventListener_start,
    &_SharedEventListener_destroy,
    &_WMTGetOSVersion,
    &_MTLDevice_newBinaryArchive,
    &_MTLBinaryArchive_serialize,
    &_DispatchData_alloc_init,
    &_CacheReader_alloc_init,
    &_CacheReader_get,
    &_CacheWriter_alloc_init,
    &_CacheWriter_set,
    &_WMTSetMetalShaderCachePath,
    &_MTLDevice_newSharedTexture,
    &_WMTBootstrapRegister,
    &_WMTBootstrapLookUp,
    &_MTLSharedEvent_createMachPort,
    &_MTLDevice_newSharedEventWithMachPort,
    &_MTLDevice_registryID,
    &_MTLSharedEvent_waitUntilSignaledValue,
    &_MTLCounterSampleBuffer_newTimestampBuffer,
    &_MTLCounterSampleBuffer_resolveCounterRange,
    &_MTLCommandBuffer_blitCommandEncoderWithSampleBuffers,
    &_MTLCommandBuffer_property,
    &_MTLDevice_newTileRenderPipelineState,
    &_MTLDevice_newResidencySet,
    &_MTLResidencySet_addAllocations,
    &_MTLResidencySet_removeAllocations,
    &_MTLResidencySet_removeAllAllocations,
    &_MTLResidencySet_commit,
    &_MTLCommandQueue_addResidencySet,
    &_MTLDevice_newHeap,
    &_MTLDevice_heapBufferSizeAndAlign,
    &_MTLDevice_heapTextureSizeAndAlign,
    &_MTLHeap_newBuffer,
    &_MTLHeap_newTexture,
    &_MTLDevice_newIndirectCommandBuffer,
    &_MTLDevice_newLibraryWithSource,
    &_MTLTexture_getBytes,
    &_MTLSharedEvent_setWin32EventAtValues,
    &_MTLCommandBuffer_computeCommandEncoderWithSampleBuffers,
    &_MTLDevice_sampleTimestamps,
    &_DispatchData_copyBytes,
    &_MTLDevice_temporalScalerScaleRange,
    &_MTLDevice_newFrameInterpolator,
    &_MTLCommandBuffer_encodeFrameInterpolate,
};
#endif
