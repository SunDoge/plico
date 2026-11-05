fn plico_material(material: Material) -> vec4f {
    return mix(material.color, material.color2, clamp(material.uv.y, 0., 1.));
}
