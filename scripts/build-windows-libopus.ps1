#!/usr/bin/env pwsh
# Build a static libopus.a for the Windows GNU (MinGW) target from the Opus
# source vendored inside the audiopus_sys crate.
#
# Why this exists: sdkwork-aiot-adapter-xiaozhi depends on `audiopus`, whose
# `audiopus_sys` build script ships a Unix-only source build (`cp`, `sh
# autogen.sh`, `make`) that cannot run on Windows. Its documented escape hatch
# is `OPUS_LIB_DIR`/`LIBOPUS_LIB_DIR`, which links a prebuilt library and skips
# that path entirely. This script produces that prebuilt library using only
# tools already on the host (MinGW gcc/ar + CMake), so no autotools or MSYS2
# installation is required.
#
# Usage: pwsh -File scripts/build-windows-libopus.ps1 [-Force]
# Output: target/prebuilt-opus/lib/libopus.a (+ include/, + build.json)

[CmdletBinding()]
param(
    [switch]$Force,
    # Build the SILK fixed-point codec instead of the default floating-point one.
    [switch]$FixedPoint
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$outputRoot = Join-Path $repoRoot 'target/prebuilt-opus'
$libDestination = Join-Path $outputRoot 'lib/libopus.a'
$workRoot = Join-Path $repoRoot 'target/prebuilt-opus-build'

$crateName = 'audiopus_sys-0.1.8'
# The registry cache directory is named after the index in use
# (`index.crates.io-*` on CI, a mirror such as `rsproxy.cn-*` on a developer
# machine), so search them all instead of pinning one mirror. The caller may
# also hand over the archive it already resolved.
if ($env:AUDIOPUS_CRATE_ARCHIVE) {
    $crateArchive = $env:AUDIOPUS_CRATE_ARCHIVE
} else {
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $crateArchive = Get-ChildItem -Path (Join-Path $cargoHome 'registry/cache') -Filter "$crateName.crate" `
        -Recurse -File -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}

function Resolve-Tool {
    param([string]$Name, [string[]]$Candidates)
    $onPath = Get-Command $Name -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    foreach ($candidate in $Candidates) {
        if (Test-Path $candidate) { return $candidate }
    }
    throw "Required build tool '$Name' was not found. Looked on PATH and at: $($Candidates -join ', ')"
}

if ((Test-Path $libDestination) -and -not $Force) {
    Write-Host "[libopus] already present: $libDestination (use -Force to rebuild)"
    exit 0
}

if (-not $crateArchive -or -not (Test-Path $crateArchive)) {
    throw "Vendored Opus source archive not found: $crateArchive. Run any build that needs audiopus_sys first so cargo caches the crate."
}

$make = Resolve-Tool -Name 'make' -Candidates @(
    (Join-Path $env:LOCALAPPDATA 'Microsoft/WinGet/Packages/ezwinports.make_Microsoft.Winget.Source_8wekyb3d8bbwe/bin/make.exe')
)
$gcc = Resolve-Tool -Name 'gcc' -Candidates @('D:/programs/mingw64/bin/gcc.exe')
$ar = Resolve-Tool -Name 'ar' -Candidates @('D:/programs/mingw64/bin/ar.exe')
$cmake = Resolve-Tool -Name 'cmake' -Candidates @('D:/programs/cmake/bin/cmake.exe', 'C:/Strawberry/c/bin/cmake.exe')
$sh = Resolve-Tool -Name 'sh' -Candidates @('D:/programs/Git/usr/bin/sh.exe')

# CMake invokes the make program by name, so the resolved tools must win on PATH.
$env:PATH = @(
    (Split-Path -Parent $make),
    (Split-Path -Parent $gcc),
    (Split-Path -Parent $sh),
    $env:PATH
) -join [IO.Path]::PathSeparator

Write-Host "[libopus] make  : $make"
Write-Host "[libopus] gcc   : $gcc"
Write-Host "[libopus] cmake : $cmake"

if (Test-Path $workRoot) { Remove-Item -LiteralPath $workRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $workRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $outputRoot 'lib') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $outputRoot 'include') | Out-Null

# 1. Extract the vendored Opus source. Windows' bundled bsdtar is pinned by
#    absolute path because the MSYS tar that arrives with the toolchain PATH
#    rewrites Windows paths and fails on the drive letter.
$windowsTar = Join-Path $env:SystemRoot 'system32/tar.exe'
if (-not (Test-Path $windowsTar)) { throw "Windows tar not found: $windowsTar" }
$crateArchiveForTar = $crateArchive -replace '\\', '/'
Push-Location $workRoot
try {
    & $windowsTar -xzf $crateArchiveForTar "$crateName/opus"
    if ($LASTEXITCODE -ne 0) { throw "tar extraction failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

$sourceDir = Join-Path $workRoot "$crateName/opus"
if (-not (Test-Path (Join-Path $sourceDir 'celt/celt.c'))) {
    throw "Extracted Opus source is incomplete: $sourceDir"
}

# 2. Generate opus_version.h. Autotools derives this from configure.ac; the
#    crate ships no template, so the defines Opus consumes are written directly.
$configureAc = Get-Content (Join-Path $sourceDir 'configure.ac') -Raw
$ltCurrent = [regex]::Match($configureAc, 'OPUS_LT_CURRENT=(\d+)').Groups[1].Value
$ltRevision = [regex]::Match($configureAc, 'OPUS_LT_REVISION=(\d+)').Groups[1].Value
$ltAge = [regex]::Match($configureAc, 'OPUS_LT_AGE=(\d+)').Groups[1].Value
if (-not ($ltCurrent -and $ltRevision -and $ltAge)) {
    throw 'Could not read the Opus libtool version triple from configure.ac'
}
# Opus release version that corresponds to libtool current:revision:age 7:0:7.
$versionNumber = '1.3.1'
$versionParts = $versionNumber.Split('.')
$versionHeader = @"
/* Generated by scripts/build-windows-libopus.ps1 - do not edit. */
#ifndef OPUS_VERSION_H
#define OPUS_VERSION_H
#define PACKAGE_VERSION "$versionNumber"
#define OPUS_VERSION "$versionNumber"
#define OPUS_MAJOR_VERSION $($versionParts[0])
#define OPUS_MINOR_VERSION $($versionParts[1])
#define OPUS_MICRO_VERSION $($versionParts[2])
#define OPUS_EXTRA_VERSION ""
#define OPUS_VERSION_STRING "$versionNumber"
#endif
"@
Set-Content -Path (Join-Path $sourceDir 'opus_version.h') -Value $versionHeader -Encoding ascii

# 3. Generate a build-only CMakeLists.txt. The vendored source ships only
#    autotools files, and the file lists below mirror *_sources.mk.
$celtSources = @(
    'celt/bands.c', 'celt/celt.c', 'celt/celt_encoder.c', 'celt/celt_decoder.c',
    'celt/cwrs.c', 'celt/entcode.c', 'celt/entdec.c', 'celt/entenc.c',
    'celt/kiss_fft.c', 'celt/laplace.c', 'celt/mathops.c', 'celt/mdct.c',
    'celt/modes.c', 'celt/pitch.c', 'celt/celt_lpc.c', 'celt/quant_bands.c',
    'celt/rate.c', 'celt/vq.c',
    'celt/x86/x86cpu.c', 'celt/x86/x86_celt_map.c', 'celt/x86/pitch_sse.c',
    'celt/x86/pitch_sse2.c', 'celt/x86/vq_sse2.c',
    'celt/x86/celt_lpc_sse4_1.c', 'celt/x86/pitch_sse4_1.c'
)
$silkSources = @(
    'silk/CNG.c', 'silk/code_signs.c', 'silk/init_decoder.c', 'silk/decode_core.c',
    'silk/decode_frame.c', 'silk/decode_parameters.c', 'silk/decode_indices.c',
    'silk/decode_pulses.c', 'silk/decoder_set_fs.c', 'silk/dec_API.c', 'silk/enc_API.c',
    'silk/encode_indices.c', 'silk/encode_pulses.c', 'silk/gain_quant.c',
    'silk/interpolate.c', 'silk/LP_variable_cutoff.c', 'silk/NLSF_decode.c', 'silk/NSQ.c',
    'silk/NSQ_del_dec.c', 'silk/PLC.c', 'silk/shell_coder.c', 'silk/tables_gain.c',
    'silk/tables_LTP.c', 'silk/tables_NLSF_CB_NB_MB.c', 'silk/tables_NLSF_CB_WB.c',
    'silk/tables_other.c', 'silk/tables_pitch_lag.c', 'silk/tables_pulses_per_block.c',
    'silk/VAD.c', 'silk/control_audio_bandwidth.c', 'silk/quant_LTP_gains.c',
    'silk/VQ_WMat_EC.c', 'silk/HP_variable_cutoff.c', 'silk/NLSF_encode.c', 'silk/NLSF_VQ.c',
    'silk/NLSF_unpack.c', 'silk/NLSF_del_dec_quant.c', 'silk/process_NLSFs.c',
    'silk/stereo_LR_to_MS.c', 'silk/stereo_MS_to_LR.c', 'silk/check_control_input.c',
    'silk/control_SNR.c', 'silk/init_encoder.c', 'silk/control_codec.c', 'silk/A2NLSF.c',
    'silk/ana_filt_bank_1.c', 'silk/biquad_alt.c', 'silk/bwexpander_32.c', 'silk/bwexpander.c',
    'silk/debug.c', 'silk/decode_pitch.c', 'silk/inner_prod_aligned.c', 'silk/lin2log.c',
    'silk/log2lin.c', 'silk/LPC_analysis_filter.c', 'silk/LPC_inv_pred_gain.c',
    'silk/table_LSF_cos.c', 'silk/NLSF2A.c', 'silk/NLSF_stabilize.c',
    'silk/NLSF_VQ_weights_laroia.c', 'silk/pitch_est_tables.c', 'silk/resampler.c',
    'silk/resampler_down2_3.c', 'silk/resampler_down2.c', 'silk/resampler_private_AR2.c',
    'silk/resampler_private_down_FIR.c', 'silk/resampler_private_IIR_FIR.c',
    'silk/resampler_private_up2_HQ.c', 'silk/resampler_rom.c', 'silk/sigm_Q15.c', 'silk/sort.c',
    'silk/sum_sqr_shift.c', 'silk/stereo_decode_pred.c', 'silk/stereo_encode_pred.c',
    'silk/stereo_find_predictor.c', 'silk/stereo_quant_pred.c', 'silk/LPC_fit.c',
    'silk/x86/NSQ_sse4_1.c', 'silk/x86/NSQ_del_dec_sse4_1.c', 'silk/x86/x86_silk_map.c',
    'silk/x86/VAD_sse4_1.c', 'silk/x86/VQ_WMat_EC_sse4_1.c'
)
# Opus compiles exactly one SILK arithmetic variant: the floating-point codec by
# default, or the fixed-point codec when FIXED_POINT is defined. audiopus_sys
# links against whichever variant the archive provides, so taking both would
# duplicate symbols and taking neither leaves enc_API.c's references undefined.
$silkFloatSources = @(
    'silk/float/apply_sine_window_FLP.c', 'silk/float/corrMatrix_FLP.c',
    'silk/float/encode_frame_FLP.c', 'silk/float/find_LPC_FLP.c', 'silk/float/find_LTP_FLP.c',
    'silk/float/find_pitch_lags_FLP.c', 'silk/float/find_pred_coefs_FLP.c',
    'silk/float/LPC_analysis_filter_FLP.c', 'silk/float/LTP_analysis_filter_FLP.c',
    'silk/float/LTP_scale_ctrl_FLP.c', 'silk/float/noise_shape_analysis_FLP.c',
    'silk/float/process_gains_FLP.c', 'silk/float/regularize_correlations_FLP.c',
    'silk/float/residual_energy_FLP.c', 'silk/float/warped_autocorrelation_FLP.c',
    'silk/float/wrappers_FLP.c', 'silk/float/autocorrelation_FLP.c',
    'silk/float/burg_modified_FLP.c', 'silk/float/bwexpander_FLP.c', 'silk/float/energy_FLP.c',
    'silk/float/inner_product_FLP.c', 'silk/float/k2a_FLP.c', 'silk/float/LPC_inv_pred_gain_FLP.c',
    'silk/float/pitch_analysis_core_FLP.c', 'silk/float/scale_copy_vector_FLP.c',
    'silk/float/scale_vector_FLP.c', 'silk/float/schur_FLP.c', 'silk/float/sort_FLP.c'
)
$silkFixedSources = @(
    'silk/fixed/LTP_analysis_filter_FIX.c', 'silk/fixed/LTP_scale_ctrl_FIX.c',
    'silk/fixed/corrMatrix_FIX.c', 'silk/fixed/encode_frame_FIX.c', 'silk/fixed/find_LPC_FIX.c',
    'silk/fixed/find_LTP_FIX.c', 'silk/fixed/find_pitch_lags_FIX.c',
    'silk/fixed/find_pred_coefs_FIX.c', 'silk/fixed/noise_shape_analysis_FIX.c',
    'silk/fixed/process_gains_FIX.c', 'silk/fixed/regularize_correlations_FIX.c',
    'silk/fixed/residual_energy16_FIX.c', 'silk/fixed/residual_energy_FIX.c',
    'silk/fixed/warped_autocorrelation_FIX.c', 'silk/fixed/apply_sine_window_FIX.c',
    'silk/fixed/autocorr_FIX.c', 'silk/fixed/burg_modified_FIX.c', 'silk/fixed/k2a_FIX.c',
    'silk/fixed/k2a_Q16_FIX.c', 'silk/fixed/pitch_analysis_core_FIX.c',
    'silk/fixed/vector_ops_FIX.c', 'silk/fixed/schur64_FIX.c', 'silk/fixed/schur_FIX.c',
    'silk/fixed/x86/vector_ops_FIX_sse4_1.c', 'silk/fixed/x86/burg_modified_FIX_sse4_1.c'
)
$silkSources += if ($FixedPoint) { $silkFixedSources } else { $silkFloatSources }
$opusSources = @(
    'src/opus.c', 'src/opus_decoder.c', 'src/opus_encoder.c', 'src/opus_multistream.c',
    'src/opus_multistream_encoder.c', 'src/opus_multistream_decoder.c', 'src/repacketizer.c',
    'src/opus_projection_encoder.c', 'src/opus_projection_decoder.c', 'src/mapping_matrix.c',
    'src/analysis.c', 'src/mlp.c', 'src/mlp_data.c'
)

$quotedSources = ($celtSources + $silkSources + $opusSources) |
    ForEach-Object { "    `"$_`"" }
$fixedPointDefinition = if ($FixedPoint) { '    FIXED_POINT=1' } else { '' }
$codecVariant = if ($FixedPoint) { 'fixed-point' } else { 'floating-point' }
Write-Host "[libopus] SILK codec variant: $codecVariant ($($silkSources.Count + $celtSources.Count + $opusSources.Count) translation units)"
$cmakeLists = @"
cmake_minimum_required(VERSION 3.15)
project(opus C)

set(CMAKE_C_STANDARD 99)

add_library(opus STATIC
$($quotedSources -join "`n")
)

target_include_directories(opus PUBLIC
    `"`${CMAKE_CURRENT_SOURCE_DIR}`"
    `"`${CMAKE_CURRENT_SOURCE_DIR}/include`"
    `"`${CMAKE_CURRENT_SOURCE_DIR}/silk`"
    `"`${CMAKE_CURRENT_SOURCE_DIR}/silk/float`"
    `"`${CMAKE_CURRENT_SOURCE_DIR}/silk/fixed`"
    `"`${CMAKE_CURRENT_SOURCE_DIR}/celt`"
    `"`${CMAKE_CURRENT_SOURCE_DIR}/src`"
)

target_compile_definitions(opus PUBLIC
    OPUS_BUILD=1
    VAR_ARRAYS=1
    OPUS_X86_MAY_HAVE_SSE=1
    OPUS_X86_MAY_HAVE_SSE2=1
    OPUS_X86_MAY_HAVE_SSE4_1=1
    OPUS_X86_PRESUME_SSE=1
    OPUS_X86_PRESUME_SSE2=1
    OPUS_HAVE_RTCD=1
$($fixedPointDefinition)
)

if(CMAKE_C_COMPILER_ID STREQUAL "GNU")
    target_compile_options(opus PRIVATE -msse4.1)
endif()

if(NOT DEFINED CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()
"@
Set-Content -Path (Join-Path $sourceDir 'CMakeLists.txt') -Value $cmakeLists -Encoding ascii

# 4. Configure and build.
$buildDir = Join-Path $workRoot 'build'
Write-Host '[libopus] configuring (MinGW Makefiles)'
& $cmake -S $sourceDir -B $buildDir -G 'MinGW Makefiles' -DCMAKE_BUILD_TYPE=Release -DCMAKE_MAKE_PROGRAM="$make"
if ($LASTEXITCODE -ne 0) { throw "cmake configure failed with exit code $LASTEXITCODE" }

Write-Host '[libopus] compiling'
& $cmake --build $buildDir --parallel
if ($LASTEXITCODE -ne 0) { throw "cmake build failed with exit code $LASTEXITCODE" }

$builtLibrary = Get-ChildItem -Path $buildDir -Filter 'libopus.a' -Recurse -File |
    Select-Object -First 1
if (-not $builtLibrary) { throw "libopus.a was not produced under $buildDir" }

Copy-Item -LiteralPath $builtLibrary.FullName -Destination $libDestination -Force
Copy-Item -Path (Join-Path $sourceDir 'include/*') -Destination (Join-Path $outputRoot 'include') -Recurse -Force

$manifest = [ordered]@{
    target = 'x86_64-pc-windows-gnu'
    opusVersion = $versionNumber
    libtoolVersion = "$ltCurrent`:$ltRevision`:$ltAge"
    silkCodecVariant = $codecVariant
    source = $crateArchive
    builtAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    library = $libDestination
    libraryBytes = (Get-Item $libDestination).Length
}
$manifest | ConvertTo-Json | Set-Content -Path (Join-Path $outputRoot 'build.json') -Encoding utf8

Write-Host "[libopus] built $libDestination ($($manifest.libraryBytes) bytes)"
Write-Host '[libopus] done'
