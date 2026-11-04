// Copyright (c) Microsoft Corporation. All rights reserved.
// SPDX-License-Identifier: MIT
// Portable oracle using the original Microsoft Terminal functions at commit
// 1283c0f5b99a2961673249fa77c6b986efb5086c, not the Rust translation.
// Sources: src/renderer/atlas/dwrite.cpp and dwrite.hlsl.
// License: ../../licenses/microsoft-terminal-MIT.txt.
// Optional manual regeneration (never built by Cargo/CI):
// c++ -std=c++17 -O2 directwrite-correction-reference.cpp -o /tmp/dwrite-reference
#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
template<class T> T clamp(T v,T lo,T hi){return std::clamp(v,lo,hi);}
struct float3 {float x,y,z; float3(float a,float b,float c):x(a),y(b),z(c){};
    float3 operator*(float k) const {return {x*k,y*k,z*k};}};
struct float4 {float x,y,z,w;};
float dot(float3 a,float3 b){return a.x*b.x+a.y*b.y+a.z*b.z;}
float saturate(float a){return std::clamp(a,0.f,1.f);}
void DWrite_GetGammaRatios(float gamma, float (&out)[4]) noexcept
{
    static constexpr float gammaIncorrectTargetRatios[13][4]{
        { 0.0000f / 4.f, 0.0000f / 4.f, 0.0000f / 4.f, 0.0000f / 4.f }, // gamma = 1.0
        { 0.0166f / 4.f, -0.0807f / 4.f, 0.2227f / 4.f, -0.0751f / 4.f }, // gamma = 1.1
        { 0.0350f / 4.f, -0.1760f / 4.f, 0.4325f / 4.f, -0.1370f / 4.f }, // gamma = 1.2
        { 0.0543f / 4.f, -0.2821f / 4.f, 0.6302f / 4.f, -0.1876f / 4.f }, // gamma = 1.3
        { 0.0739f / 4.f, -0.3963f / 4.f, 0.8167f / 4.f, -0.2287f / 4.f }, // gamma = 1.4
        { 0.0933f / 4.f, -0.5161f / 4.f, 0.9926f / 4.f, -0.2616f / 4.f }, // gamma = 1.5
        { 0.1121f / 4.f, -0.6395f / 4.f, 1.1588f / 4.f, -0.2877f / 4.f }, // gamma = 1.6
        { 0.1300f / 4.f, -0.7649f / 4.f, 1.3159f / 4.f, -0.3080f / 4.f }, // gamma = 1.7
        { 0.1469f / 4.f, -0.8911f / 4.f, 1.4644f / 4.f, -0.3234f / 4.f }, // gamma = 1.8
        { 0.1627f / 4.f, -1.0170f / 4.f, 1.6051f / 4.f, -0.3347f / 4.f }, // gamma = 1.9
        { 0.1773f / 4.f, -1.1420f / 4.f, 1.7385f / 4.f, -0.3426f / 4.f }, // gamma = 2.0
        { 0.1908f / 4.f, -1.2652f / 4.f, 1.8650f / 4.f, -0.3476f / 4.f }, // gamma = 2.1
        { 0.2031f / 4.f, -1.3864f / 4.f, 1.9851f / 4.f, -0.3501f / 4.f }, // gamma = 2.2
    };
    static constexpr auto norm13 = static_cast<float>(static_cast<double>(0x10000) / (255 * 255) * 4);
    static constexpr auto norm24 = static_cast<float>(static_cast<double>(0x100) / (255) * 4);

#pragma warning(suppress : 26451) // Arithmetic overflow: Using operator '+' on a 4 byte value and then casting the result to a 8 byte value.
    const auto index = clamp<ptrdiff_t>(static_cast<ptrdiff_t>(gamma * 10.0f + 0.5f), 10, 22) - 10;
#pragma warning(suppress : 26446) // Prefer to use gsl::at() instead of unchecked subscript operator (bounds.4).
#pragma warning(suppress : 26482) // Only index into arrays using constant expressions (bounds.2).
    const auto& ratios = gammaIncorrectTargetRatios[index];

    out[0] = norm13 * ratios[0];
    out[1] = norm24 * ratios[1];
    out[2] = norm13 * ratios[2];
    out[3] = norm24 * ratios[3];
}

float DWrite_ApplyLightOnDarkContrastAdjustment(float grayscaleEnhancedContrast, float3 color)
{
    // The following 1 line is the same as this direct translation of the
    // original code, but simplified to reduce the number of instructions:
    //   float lightness = dot(color, float3(0.30f, 0.59f, 0.11f);
    //   float multiplier = saturate(4.0f * (0.75f - lightness));
    //   return grayscaleEnhancedContrast * multiplier;
    return grayscaleEnhancedContrast * saturate(dot(color, float3(0.30f, 0.59f, 0.11f) * -4.0f) + 3.0f);
}

float DWrite_CalcColorIntensity(float3 color)
{
    return dot(color, float3(0.25f, 0.5f, 0.25f));
}

float DWrite_EnhanceContrast(float alpha, float k)
{
    return alpha * (k + 1.0f) / (alpha * k + 1.0f);
}

float DWrite_ApplyAlphaCorrection(float a, float f, float4 g)
{
    return a + a * (1.0f - a) * ((g.x * f + g.y) * a + (g.z * f + g.w));
}
int main(){
    struct Sample {float gamma,contrast;float3 color;float opacity;unsigned coverage;bool thin;};
    const Sample samples[]={
        {1.f,0.f,{.2f,.4f,.8f},.5f,128,false},
        {1.8f,1.f,{0.f,0.f,0.f},1.f,128,false},
        {1.8f,1.f,{1.f,1.f,1.f},1.f,128,false},
        {1.8f,1.f,{1.f,0.f,0.f},1.f,128,false},
        {1.8f,1.f,{0.f,1.f,0.f},1.f,128,false},
        {1.8f,1.f,{0.f,0.f,1.f},.5f,128,false},
        {2.2f,.5f,{.1f,.7f,.3f},.4f,64,true},
        {1.1f,10.f,{.9f,.2f,.1f},1.f,200,false},
        {2.2f,10.f,{1.f,1.f,1.f},1.f,1,true},
        {1.8f,1.f,{.3f,.5f,.8f},0.f,200,true},
    };
    for(const auto& s:samples){
        float ratios[4];DWrite_GetGammaRatios(s.gamma,ratios);
        const float k=(s.thin?.5f:0.f)+DWrite_ApplyLightOnDarkContrastAdjustment(s.contrast,s.color);
        const float a=DWrite_EnhanceContrast(s.coverage/255.f,k);
        const float corrected=DWrite_ApplyAlphaCorrection(a,DWrite_CalcColorIntensity(s.color),{ratios[0],ratios[1],ratios[2],ratios[3]});
        const float alpha=saturate(corrected)*s.opacity;
        std::printf("[%u, %u, %u, %u],\n",(unsigned)std::round(s.color.x*alpha*255.f),
            (unsigned)std::round(s.color.y*alpha*255.f),(unsigned)std::round(s.color.z*alpha*255.f),(unsigned)std::round(alpha*255.f));
    }
}
