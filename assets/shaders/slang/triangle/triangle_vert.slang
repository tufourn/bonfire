cbuffer Uniforms
{
    float4x4 modelViewProjection;
}

struct AssembledVertex
{
    float3	position : POSITION;
    float3	color    : COLOR;
};

struct CoarseVertex
{
    float3 color;
};
;

struct VertexStageOutput
{
    CoarseVertex    coarseVertex    : CoarseVertex;
    float4          sv_position     : SV_Position;
};

[shader("vertex")]
VertexStageOutput main(
    AssembledVertex assembledVertex)
{
    VertexStageOutput output;

    float3 position = assembledVertex.position;
    float3 color    = assembledVertex.color;

    output.coarseVertex.color = color;
    output.sv_position = mul(modelViewProjection, float4(position, 1.0));

    return output;
}
