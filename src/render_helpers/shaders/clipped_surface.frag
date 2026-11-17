#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

uniform float clarion_scale;

uniform vec2 geo_size;
uniform vec4 corner_radius;
uniform mat3 input_to_geo;
#define HAS_ROUNDING_POWER
uniform float rounding_power;

// When enabled (background effects only), multiply the result by the surface's
// per-pixel alpha so blur arrives and leaves with the buffer instead of its
// region. Inactive (uniform defaults to 0) for every other draw.
uniform float mask_enabled;
uniform mat3 input_to_mask;
uniform sampler2D mask_tex;

float clarion_rounding_alpha(vec2 coords, vec2 size, vec4 corner_radius);
vec4 postprocess(vec4 color);
vec4 clarion_sample_background(vec2 uv);

void main() {
    vec3 coords_geo = input_to_geo * vec3(v_coords, 1.0);

    // Sample the texture.
    vec4 color = clarion_sample_background(v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    color = postprocess(color);

    if (coords_geo.x < 0.0 || 1.0 < coords_geo.x || coords_geo.y < 0.0 || 1.0 < coords_geo.y) {
        // Clip outside geometry.
        color = vec4(0.0);
    } else {
        // Apply corner rounding inside geometry.
        color = color * clarion_rounding_alpha(coords_geo.xy * geo_size, geo_size, corner_radius);
    }

    // Apply final alpha and tint.
    color = color * alpha;

    if (mask_enabled > 0.5) {
        vec2 mask_uv = (input_to_mask * vec3(coords_geo.xy, 1.0)).xy;
        float mask_a = texture2D(mask_tex, clamp(mask_uv, 0.0, 1.0)).a;
        // Normalize the card's translucent fill to full coverage; a fading card
        // then scales the blur down with it instead of leaving a block behind.
        color = color * clamp(mask_a * (1.0 / 0.6), 0.0, 1.0);
    }

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
