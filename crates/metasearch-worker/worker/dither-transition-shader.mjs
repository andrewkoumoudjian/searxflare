// Based on the Paper Shaders dithering shader, adapted to morph two fields
// through one dither pass on a single WebGL canvas.
export const ditherTransitionFragmentShader = `#version 300 es
precision mediump float;

uniform float u_time;
uniform vec2 u_resolution;
uniform float u_pixelRatio;

uniform vec4 u_colorBack;
uniform vec4 u_colorFronts[5];
uniform vec4 u_params[5];
uniform vec2 u_offsets[5];
uniform vec2 u_motion[5];
uniform vec3 u_masks[5];
uniform vec4 u_weights;
uniform float u_dktWeight;
uniform float u_transitionStyle;
uniform float u_transitionProgress;
uniform float u_targetIndex;
uniform vec2 u_transitionOrigin;

out vec4 fragColor;

#define TWO_PI 6.28318530718
#define PI 3.14159265358979323846

float hash11(float p) {
  p = fract(p * 0.3183099) + 0.1;
  p *= p + 19.19;
  return fract(p * p);
}

float hash21(vec2 p) {
  p = fract(p * vec2(0.3183099, 0.3678794)) + 0.1;
  p += dot(p, p + 19.19);
  return fract(p.x * p.y);
}

vec3 permute(vec3 x) {
  return mod(((x * 34.0) + 1.0) * x, 289.0);
}

float snoise(vec2 v) {
  const vec4 C = vec4(
    0.211324865405187,
    0.366025403784439,
    -0.577350269189626,
    0.024390243902439
  );
  vec2 i = floor(v + dot(v, C.yy));
  vec2 x0 = v - i + dot(i, C.xx);
  vec2 i1 = x0.x > x0.y ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
  vec4 x12 = x0.xyxy + C.xxzz;
  x12.xy -= i1;
  i = mod(i, 289.0);
  vec3 p = permute(
    permute(i.y + vec3(0.0, i1.y, 1.0))
      + i.x
      + vec3(0.0, i1.x, 1.0)
  );
  vec3 m = max(
    0.5 - vec3(
      dot(x0, x0),
      dot(x12.xy, x12.xy),
      dot(x12.zw, x12.zw)
    ),
    0.0
  );
  m = m * m;
  m = m * m;
  vec3 x = 2.0 * fract(p * C.www) - 1.0;
  vec3 h = abs(x) - 0.5;
  vec3 ox = floor(x + 0.5);
  vec3 a0 = x - ox;
  m *= 1.79284291400159
    - 0.85373472095314 * (a0 * a0 + h * h);
  vec3 g;
  g.x = a0.x * x0.x + h.x * x0.y;
  g.yz = a0.yz * x12.xz + h.yz * x12.yw;
  return 130.0 * dot(m, g);
}

float getSimplexNoise(vec2 uv, float t) {
  float noise = 0.5 * snoise(uv - vec2(0.0, 0.3 * t));
  noise += 0.5 * snoise(2.0 * uv + vec2(0.0, 0.32 * t));
  return noise;
}

vec2 rotateUv(vec2 uv, float degrees) {
  float radians = degrees * PI / 180.0;
  mat2 rotation = mat2(
    cos(radians),
    sin(radians),
    -sin(radians),
    cos(radians)
  );
  return rotation * uv;
}

vec2 getShapeUv(
  vec2 normalizedUv,
  float shapeType,
  float scale,
  float rotation,
  vec2 offset
) {
  vec2 shapeUv = normalizedUv;

  if (shapeType > 3.5) {
    float objectSize = min(u_resolution.x, u_resolution.y);
    vec2 worldScale = u_resolution / objectSize;
    shapeUv *= worldScale;
    shapeUv += vec2(-offset.x, offset.y);
    shapeUv /= scale;
    return rotateUv(shapeUv, rotation);
  }

  float patternSize = min(u_resolution.x, u_resolution.y);
  vec2 worldScale = u_resolution / patternSize;
  shapeUv += vec2(-offset.x, offset.y) / worldScale;
  shapeUv *= u_resolution;
  shapeUv /= u_pixelRatio;
  shapeUv /= scale;
  shapeUv = rotateUv(shapeUv, rotation);
  return shapeUv + 0.5;
}

float evaluateShape(vec2 shapeUv, float shapeType, float t) {
  if (shapeType < 1.5) {
    shapeUv *= 0.001;
    float shape = 0.5 + 0.5 * getSimplexNoise(shapeUv, t);
    return smoothstep(0.3, 0.9, shape);
  }

  if (shapeType < 2.5) {
    shapeUv *= 0.003;
    for (float i = 1.0; i < 6.0; i++) {
      shapeUv.x += 0.6 / i * cos(i * 2.5 * shapeUv.y + t);
      shapeUv.y += 0.6 / i * cos(i * 1.5 * shapeUv.x + t);
    }
    float shape = 0.15
      / max(0.001, abs(sin(t - shapeUv.y - shapeUv.x)));
    return smoothstep(0.02, 1.0, shape);
  }

  if (shapeType < 3.5) {
    shapeUv *= 0.05;
    float stripeIndex = floor(2.0 * shapeUv.x / TWO_PI);
    float randomValue = hash11(stripeIndex * 10.0);
    randomValue = sign(randomValue - 0.5)
      * pow(0.1 + abs(randomValue), 0.4);
    float shape = sin(shapeUv.x)
      * cos(shapeUv.y - 5.0 * randomValue * t);
    return pow(abs(shape), 6.0);
  }

  if (shapeType < 4.5) {
    shapeUv *= 4.0;
    float wave = cos(0.5 * shapeUv.x - 2.0 * t)
      * sin(1.5 * shapeUv.x + t)
      * (0.75 + 0.25 * cos(3.0 * t));
    return 1.0 - smoothstep(-1.0, 1.0, shapeUv.y + wave);
  }

  if (shapeType < 5.5) {
    float distanceFromCenter = length(shapeUv);
    return 0.5 + 0.5 * sin(
      pow(distanceFromCenter, 1.7) * 7.0 - 3.0 * t
    );
  }

  if (shapeType < 6.5) {
    float radius = length(shapeUv);
    float angle = 6.0 * atan(shapeUv.y, shapeUv.x) + 4.0 * t;
    float twist = 1.2;
    float offset = 1.0 / pow(max(radius, 1e-6), twist)
      + angle / TWO_PI;
    float middle = smoothstep(0.0, 1.0, pow(radius, twist));
    return mix(0.0, fract(offset), middle);
  }

  shapeUv *= 2.0;
  float depth = 1.0 - pow(length(shapeUv), 2.0);
  vec3 position = vec3(shapeUv, sqrt(max(0.0, depth)));
  vec3 lightPosition = normalize(
    vec3(cos(1.5 * t), 0.8, sin(1.25 * t))
  );
  float shape = 0.5 + 0.5 * dot(lightPosition, position);
  return shape * step(0.0, depth);
}

float getDitherThreshold(vec2 pixelUv) {
  vec2 cell = floor(pixelUv);
  return fract(
    cell.x * 0.754877666 + cell.y * 0.569840296
  );
}

float roundedBoxDistance(vec2 point, vec2 halfSize, float radius) {
  vec2 edge = abs(point) - halfSize + radius;
  return length(max(edge, 0.0))
    + min(max(edge.x, edge.y), 0.0)
    - radius;
}

float getCardField(
  vec2 screenUv,
  vec2 center,
  vec2 halfSize,
  float radius,
  float rotation,
  float softness
) {
  vec2 point = screenUv - center;
  point.x *= u_resolution.x / u_resolution.y;
  point = rotateUv(point, rotation);
  float distanceToEdge = roundedBoxDistance(
    point,
    halfSize,
    radius
  );
  return 1.0 - smoothstep(
    -softness,
    softness * 0.4,
    distanceToEdge
  );
}

float getDktFieldShape(
  vec2 screenUv,
  float fieldStyle,
  float fallbackShape,
  float t
) {
  if (fieldStyle < 0.5) {
    return fallbackShape;
  }

  float paperGrain = 0.035 * snoise(screenUv * 4.0 + t * 0.08);

  if (fieldStyle < 1.5) {
    vec2 singleCenter = vec2(0.34, 0.68) + vec2(
      0.022 * sin(t * 1.7),
      0.016 * cos(t * 1.35)
    );
    float singleCard = getCardField(
      screenUv,
      singleCenter,
      vec2(0.31, 0.285),
      0.13,
      -5.0 + 2.4 * sin(t * 0.92),
      0.18
    );
    return clamp(singleCard + paperGrain, 0.0, 1.0);
  }

  if (fieldStyle < 2.5) {
    vec2 backCenter = vec2(0.28, 0.66) + vec2(
      0.032 * sin(t * 1.55),
      0.02 * cos(t * 1.18)
    );
    vec2 frontCenter = vec2(0.43, 0.7) + vec2(
      0.028 * cos(t * 1.34 + 1.1),
      0.022 * sin(t * 1.62 + 0.4)
    );
    float backCard = 0.88 * getCardField(
      screenUv,
      backCenter,
      vec2(0.25, 0.275),
      0.12,
      -11.0 + 3.2 * sin(t * 0.86),
      0.18
    );
    float frontCard = getCardField(
      screenUv,
      frontCenter,
      vec2(0.255, 0.285),
      0.125,
      4.0 + 3.0 * cos(t * 0.78 + 0.6),
      0.18
    );
    return clamp(max(backCard, frontCard) + paperGrain, 0.0, 1.0);
  }

  vec2 topLeftCenter = vec2(0.08, 0.18) + vec2(
    0.018 * sin(t * 1.31 + 0.3) + 0.008 * sin(t * 2.17 + 1.4),
    0.016 * cos(t * 1.07 + 0.9) + 0.007 * sin(t * 1.83)
  );
  vec2 midRightCenter = vec2(0.91, 0.52) + vec2(
    0.022 * cos(t * 1.43 + 2.1) + 0.007 * sin(t * 2.03),
    0.018 * sin(t * 1.19 + 1.2) + 0.006 * cos(t * 1.77)
  );
  vec2 bottomLeftCenter = vec2(0.16, 0.87) + vec2(
    0.02 * sin(t * 1.57 + 2.6) + 0.006 * cos(t * 2.21),
    0.016 * cos(t * 1.29 + 0.4) + 0.006 * sin(t * 1.91)
  );
  float topLeftCard = 0.86 * getCardField(
    screenUv,
    topLeftCenter,
    vec2(0.13, 0.17),
    0.07,
    -4.0 + 1.8 * sin(t * 0.74 + 0.5),
    0.042
  );
  float midRightCard = getCardField(
    screenUv,
    midRightCenter,
    vec2(0.15, 0.19),
    0.078,
    3.0 + 1.8 * cos(t * 0.82),
    0.048
  );
  float bottomLeftCard = 0.82 * getCardField(
    screenUv,
    bottomLeftCenter,
    vec2(0.14, 0.17),
    0.07,
    5.0 + 1.7 * sin(t * 0.9 + 1.2),
    0.044
  );
  float broadFlow = 0.5 + 0.5 * snoise(
    screenUv * 7.0 + vec2(t * 2.0, -t * 1.45)
  );
  float fineFlow = 0.5 + 0.5 * snoise(
    screenUv * 14.0 + vec2(-t * 2.7, t * 1.9) + vec2(8.0, 3.0)
  );
  float internalFlow = 0.85 + 0.1 * broadFlow + 0.05 * fineFlow;
  float cardCluster = max(
    topLeftCard,
    max(midRightCard, bottomLeftCard)
  );
  return clamp(
    cardCluster * internalFlow + 0.12 * paperGrain,
    0.0,
    1.0
  );
}

float getFieldMask(vec2 screenUv, vec3 maskData) {
  vec2 distanceFromCenter = screenUv - maskData.xy;
  distanceFromCenter.x *= 0.78;
  float distanceValue = length(distanceFromCenter);
  return 1.0 - smoothstep(0.0, maskData.z, distanceValue);
}

float getSpatialTransitionGate(
  vec2 screenUv,
  vec2 pixelUv
) {
  float progress = clamp(u_transitionProgress, 0.0, 1.0);
  if (progress <= 0.001) {
    return 0.0;
  }
  if (progress >= 0.999) {
    return 1.0;
  }

  vec2 cell = floor(pixelUv);
  float cellNoise = hash21(cell + vec2(17.0, 43.0));

  if (u_transitionStyle < 2.5) {
    return smoothstep(
      cellNoise - 0.08,
      cellNoise + 0.08,
      progress
    );
  }

  if (u_transitionStyle < 3.5) {
    float radius = mix(-0.12, 1.5, progress);
    float distanceFromOrigin = distance(screenUv, u_transitionOrigin);
    float noisyDistance = distanceFromOrigin + (cellNoise - 0.5) * 0.09;
    return 1.0 - smoothstep(
      radius - 0.07,
      radius + 0.07,
      noisyDistance
    );
  }

  if (u_transitionStyle < 4.5) {
    float sweepPosition = 0.5 * (
      screenUv.x + 1.0 - screenUv.y
    );
    float sweepEdge = mix(-0.12, 1.12, progress);
    float noisyPosition = sweepPosition + (cellNoise - 0.5) * 0.08;
    return 1.0 - smoothstep(
      sweepEdge - 0.055,
      sweepEdge + 0.055,
      noisyPosition
    );
  }

  if (u_transitionStyle < 5.5) {
    vec2 vortexUv = screenUv - u_transitionOrigin;
    vortexUv.x *= u_resolution.x / u_resolution.y;
    float vortexRadius = length(vortexUv);
    float vortexAngle = atan(vortexUv.y, vortexUv.x) / TWO_PI + 0.5;
    float vortexOrder = fract(
      vortexAngle + vortexRadius * 1.85 + cellNoise * 0.035
    );
    return smoothstep(
      vortexOrder - 0.045,
      vortexOrder + 0.045,
      progress
    );
  }

  if (u_transitionStyle < 6.5) {
    float scanline = floor(screenUv.y * 34.0);
    float bandShift = hash11(scanline * 7.13) - 0.5;
    float tear = snoise(vec2(scanline * 0.08, progress * 3.0));
    float signalOrder = clamp(
      screenUv.x + bandShift * 0.72 + tear * 0.16,
      0.0,
      1.0
    );
    float signalGate = smoothstep(
      signalOrder - 0.025,
      signalOrder + 0.025,
      progress
    );
    float dropout = step(
      0.94,
      hash21(vec2(scanline, floor(progress * 18.0)))
    );
    return mix(signalGate, 1.0 - signalGate, dropout);
  }

  vec2 floodUv = screenUv * vec2(
    u_resolution.x / u_resolution.y,
    1.0
  );
  float floodNoise = 0.5 + 0.31 * snoise(floodUv * 3.6)
    + 0.14 * snoise(floodUv * 8.5 + 4.2)
    + (cellNoise - 0.5) * 0.05;
  return smoothstep(
    floodNoise - 0.065,
    floodNoise + 0.065,
    progress
  );
}

float getVariantWeight(int index) {
  if (index < 4) {
    return u_weights[index];
  }
  return u_dktWeight;
}

void main() {
  float sharedPxSize = 0.0;
  float maxWeight = 0.0;
  for (int i = 0; i < 5; i++) {
    float weight = getVariantWeight(i);
    sharedPxSize += weight * u_params[i].y;
    maxWeight = max(maxWeight, weight);
  }
  sharedPxSize *= u_pixelRatio;
  float transitionAmount = clamp((1.0 - maxWeight) * 2.0, 0.0, 1.0);
  float coverageFeather = mix(
    0.008,
    0.085,
    transitionAmount
  );
  vec2 centeredPosition = gl_FragCoord.xy - 0.5 * u_resolution;

  vec2 sharedPixelUv = centeredPosition / sharedPxSize;
  vec2 sharedPixelPosition = (
    floor(sharedPixelUv) + 0.5
  ) * sharedPxSize;
  vec2 sharedNormalizedUv = sharedPixelPosition / u_resolution;

  vec2 screenUv = vec2(
    gl_FragCoord.x / u_resolution.x,
    1.0 - gl_FragCoord.y / u_resolution.y
  );
  float threshold = getDitherThreshold(sharedPixelUv);
  float spatialTransitionGate = getSpatialTransitionGate(
    screenUv,
    sharedPixelUv
  );

  float fieldShape = 0.0;
  float fieldMask = 0.0;
  vec3 frontLinear = vec3(0.0);
  float frontAlpha = 0.0;
  vec3 crossfadeColor = vec3(0.0);
  float crossfadeOpacity = 0.0;
  vec3 sourceColor = vec3(0.0);
  vec3 targetColor = vec3(0.0);
  float sourceOpacity = 0.0;
  float targetOpacity = 0.0;

  for (int i = 0; i < 5; i++) {
    float weight = getVariantWeight(i);
    if (weight <= 0.0001) {
      continue;
    }

    bool isTarget = abs(float(i) - u_targetIndex) < 0.5;
    float crossfadeWeight = weight;
    if (u_transitionStyle > 1.5 && u_transitionStyle < 7.5) {
      crossfadeWeight = isTarget
        ? spatialTransitionGate
        : 1.0 - spatialTransitionGate;
    }
    if (u_transitionStyle > 7.5 && u_transitionStyle < 8.5) {
      float exposure = 1.0 + 0.9 * sin(u_transitionProgress * PI);
      crossfadeWeight *= exposure;
    }

    vec4 params = u_params[i];
    vec2 motion = u_motion[i];
    vec2 sampleUv = sharedNormalizedUv;
    vec2 sampleScreenUv = screenUv;
    if (u_transitionStyle > 10.5 && u_transitionStyle < 11.5) {
      float orbitAmount = 0.14 * sin(u_transitionProgress * PI);
      float orbitAngle = u_transitionProgress * PI * 1.6;
      vec2 orbitOffset = orbitAmount * vec2(
        cos(orbitAngle),
        sin(orbitAngle)
      );
      if (!isTarget) {
        orbitOffset *= -1.0;
      }
      sampleUv += orbitOffset;
      sampleScreenUv += orbitOffset;
    }
    vec2 shapeUv = getShapeUv(
      sampleUv,
      params.x,
      params.z,
      motion.x,
      u_offsets[i]
    );
    float shapeTime = 0.5 * u_time * motion.y;
    float shape = evaluateShape(
      shapeUv,
      params.x,
      shapeTime
    );
    shape = getDktFieldShape(
      sampleScreenUv,
      params.w,
      shape,
      shapeTime
    );
    float mask = getFieldMask(sampleScreenUv, u_masks[i]);
    float coverage = smoothstep(
      threshold - coverageFeather,
      threshold + coverageFeather,
      shape
    ) * mask;
    fieldShape += weight * shape;
    fieldMask += weight * mask;

    vec4 frontColor = u_colorFronts[i];
    frontLinear += weight * pow(
      max(frontColor.rgb, vec3(0.0)),
      vec3(2.2)
    );
    frontAlpha += weight * frontColor.a;
    vec3 blendColor = frontColor.rgb;
    if (u_transitionStyle > 9.5 && u_transitionStyle < 10.5) {
      float splitAmount = 0.92 * sin(u_transitionProgress * PI);
      float luminance = dot(
        frontColor.rgb,
        vec3(0.2126, 0.7152, 0.0722)
      );
      vec3 splitTint = luminance * (
        isTarget
          ? vec3(0.12, 0.82, 1.35)
          : vec3(1.35, 0.12, 0.58)
      );
      blendColor = mix(
        blendColor,
        splitTint,
        splitAmount
      );
    }
    crossfadeColor += crossfadeWeight
      * blendColor
      * frontColor.a
      * coverage;
    crossfadeOpacity += crossfadeWeight * frontColor.a * coverage;
    if (isTarget) {
      targetColor += weight * frontColor.rgb * frontColor.a * coverage;
      targetOpacity += weight * frontColor.a * coverage;
    } else {
      sourceColor += weight * frontColor.rgb * frontColor.a * coverage;
      sourceOpacity += weight * frontColor.a * coverage;
    }
  }

  float ditheredField = smoothstep(
    threshold - coverageFeather,
    threshold + coverageFeather,
    fieldShape
  ) * fieldMask;

  vec3 frontColor = pow(
    max(frontLinear, vec3(0.0)),
    vec3(1.0 / 2.2)
  );
  vec3 foreground = frontColor * frontAlpha;
  vec3 background = u_colorBack.rgb * u_colorBack.a;
  float foregroundOpacity = frontAlpha * ditheredField;
  vec3 color = foreground * ditheredField;
  if (u_transitionStyle > 0.5) {
    color = crossfadeColor;
    foregroundOpacity = min(crossfadeOpacity, 1.0);
  }
  if (u_transitionStyle > 8.5 && u_transitionStyle < 9.5) {
    float overlap = min(sourceOpacity, targetOpacity);
    float cancellation = smoothstep(0.0, 0.42, overlap);
    color = mix(
      sourceColor + targetColor,
      abs(targetColor - sourceColor),
      0.82 * cancellation
    );
    foregroundOpacity = clamp(
      abs(targetOpacity - sourceOpacity) + overlap * 0.38,
      0.0,
      1.0
    );
  }
  color += background * (1.0 - foregroundOpacity);
  float opacity = foregroundOpacity
    + u_colorBack.a * (1.0 - foregroundOpacity);

  fragColor = vec4(color, opacity);
}
`;
