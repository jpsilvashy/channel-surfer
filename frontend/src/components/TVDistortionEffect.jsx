import React, { useEffect, useRef } from 'react';

const vertexShader = `
  attribute vec2 position;
  varying vec2 vUv;
  void main() {
    vUv = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
  }
`;

const fragmentShader = `
  precision highp float;
  uniform float time;
  uniform vec2 resolution;
  varying vec2 vUv;

  float random(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453123);
  }

  // Extreme wave and twist deformation
  vec2 extremeDeform(vec2 uv, float time) {
    vec2 deformed = uv;
    
    // Spiral twist effect
    vec2 center = vec2(0.5, 0.5);
    vec2 toCenter = uv - center;
    float dist = length(toCenter);
    float angle = atan(toCenter.y, toCenter.x);
    float twist = sin(time * 2.0) * 10.0;
    angle += dist * twist;
    float newDist = dist * (1.0 + sin(time * 3.0 + dist * 10.0) * 0.3);
    deformed = center + vec2(cos(angle), sin(angle)) * newDist;
    
    // Multiple overlapping extreme waves
    float wave1 = sin(uv.y * 20.0 + time * 4.0) * 0.1;
    float wave2 = sin(uv.y * 10.0 - time * 3.0) * 0.15;
    float wave3 = sin(uv.x * 30.0 + time * 5.0) * 0.08;
    float wave4 = sin((uv.x + uv.y) * 15.0 + time * 2.0) * 0.12;
    float wave5 = sin(dist * 30.0 - time * 6.0) * 0.1;
    
    deformed.x += wave1 + wave2 + wave3 + wave5;
    deformed.y += wave4 + wave5;
    
    // Extreme pinching effect
    float pinch = sin(time) * 0.5 + 0.5;
    deformed += normalize(toCenter) * sin(dist * 20.0 - time * 3.0) * 0.2 * pinch;
    
    // Shockwave effect
    float shockwave = sin(dist * 40.0 - time * 4.0) * 0.1;
    deformed += normalize(toCenter) * shockwave;

    return deformed;
  }

  // Enhanced chromatic aberration with RGB splitting
  vec3 extremeChromatic(vec2 uv, float time) {
    float maxOffset = 0.1; // Base chromatic aberration strength
    
    vec2 center = vec2(0.5, 0.5);
    vec2 toCenter = uv - center;
    float dist = length(toCenter);
    
    // Dynamic aberration strength that pulses and varies with screen position
    float aberrationStrength = maxOffset * (1.0 + sin(time * 2.0) * 0.3);
    aberrationStrength *= (1.0 + dist * 2.0); // Stronger at edges
    
    // Create RGB channel offsets with different patterns
    vec2 redOffset = vec2(
      cos(time * 1.1 + uv.y * 5.0) * aberrationStrength,
      sin(time * 0.8 + uv.x * 4.0) * aberrationStrength
    );
    
    vec2 greenOffset = vec2(
      cos(time * 0.9 + uv.y * 4.0) * aberrationStrength * 0.8,
      sin(time * 1.2 + uv.x * 5.0) * aberrationStrength * 0.8
    );
    
    vec2 blueOffset = vec2(
      cos(time * 1.3 + uv.y * 6.0) * aberrationStrength * 1.2,
      sin(time * 1.0 + uv.x * 3.0) * aberrationStrength * 1.2
    );
    
    // Sample colors with offsets
    vec2 redUV = uv + redOffset;
    vec2 greenUV = uv + greenOffset;
    vec2 blueUV = uv + blueOffset;
    
    // Add color bleeding and glow
    float r = random(redUV) * (1.2 + sin(time * 3.0 + uv.y * 10.0) * 0.3);
    float g = random(greenUV) * (1.1 + sin(time * 2.7 + uv.x * 8.0) * 0.2);
    float b = random(blueUV) * (1.3 + sin(time * 3.3 + dist * 12.0) * 0.4);
    
    // Add color cross-talk
    r += g * 0.1 + b * 0.05;
    g += r * 0.08 + b * 0.1;
    b += r * 0.05 + g * 0.08;
    
    return vec3(r, g, b);
  }

  // Enhanced scanlines with phosphor glow
  float getScanlines(vec2 uv, float time) {
    // Multiple layers of scanlines
    float scanlines = 0.0;
    
    // Primary scanlines
    float primaryFreq = 300.0;
    float primaryAmp = 0.15;
    scanlines += sin(uv.y * primaryFreq + time) * primaryAmp;
    
    // Secondary finer scanlines
    float secondaryFreq = 600.0;
    float secondaryAmp = 0.08;
    scanlines += sin(uv.y * secondaryFreq - time * 2.0) * secondaryAmp;
    
    // Horizontal distortion lines
    float horizontalFreq = 200.0;
    float horizontalAmp = 0.05;
    scanlines += sin(uv.x * horizontalFreq + time * 3.0) * horizontalAmp;
    
    // Add phosphor decay effect
    float decay = fract(uv.y * 3.0 + time * 0.5);
    decay = pow(decay, 3.0) * 0.2;
    scanlines += decay;
    
    // Add random noise to scanlines
    float noise = random(uv + time * 0.1) * 0.1;
    scanlines += noise;
    
    return scanlines;
  }

  // Extreme interference patterns with prominent rolling bands
  float extremeInterference(vec2 uv, float time) {
    float interference = 0.0;
    
    // Multiple interference patterns
    for(float i = 1.0; i < 5.0; i++) {
      float speed = i * 2.0;
      float scale = i * 10.0;
      interference += sin(uv.y * scale + time * speed) * (0.3 / i);
      interference += cos(uv.x * scale - time * speed) * (0.3 / i);
    }
    
    // Enhanced vertical rolling bands
    float bandSpeed = 0.8;
    float bandWidth = 0.15;
    float bandCount = 4.0;
    
    for(float i = 0.0; i < bandCount; i++) {
      float offset = i / bandCount;
      float y = mod(uv.y + time * bandSpeed + offset, 1.0);
      
      // Create smooth, dark bands with sharp edges
      float band = smoothstep(0.0, 0.02, y) * smoothstep(bandWidth, bandWidth - 0.02, y);
      interference = max(interference - band * 0.8, -0.8); // Make bands darker and more visible
      
      // Add subtle color tinting to bands
      if(band > 0.1) {
        interference += sin(time * 5.0 + uv.x * 10.0) * 0.1;
      }
    }
    
    // Add additional horizontal scan distortion within the bands
    float scanDistort = sin(uv.y * 100.0 + time * 10.0) * 0.1;
    interference += scanDistort * step(0.5, abs(interference));
    
    return interference;
  }

  void main() {
    vec2 uv = vUv;
    vec2 deformedUv = extremeDeform(uv, time);
    
    // Get base color with enhanced chromatic aberration
    vec3 color = extremeChromatic(deformedUv, time);
    
    // Add interference with rolling bands
    float interference = extremeInterference(deformedUv, time);
    color += vec3(interference) * vec3(0.6, 0.5, 0.7);
    color *= 1.0 + interference * 0.5;
    
    // Apply enhanced scanlines with phosphor glow
    float scanlines = getScanlines(deformedUv, time);
    color *= 1.0 + scanlines;
    
    // Add color-specific scanline tinting
    color.r *= 1.0 + scanlines * 0.2;
    color.g *= 1.0 + scanlines * 0.15;
    color.b *= 1.0 + scanlines * 0.25;
    
    // Add subtle color bleeding between scan lines
    float bleed = sin(deformedUv.y * 200.0 - time * 5.0) * 0.1;
    color.r += bleed * 0.1;
    color.b -= bleed * 0.08;
    
    // Add phosphor glow
    float glow = sin(time * 2.0) * 0.1 + 0.9;
    color *= glow;
    
    // Extreme scanlines that move and distort
    float scanline = sin(deformedUv.y * 1000.0 + time * 5.0) * 0.5;
    color *= 1.0 + scanline;
    
    // Random color channel inversions
    if(random(vec2(time * 0.5)) > 0.95) {
      color = 1.0 - color;
    }
    
    // Extreme color shifts
    float colorShift = sin(time * 10.0);
    if(colorShift > 0.0) {
      color.r *= 2.0;
      color.g *= 0.5;
    } else {
      color.g *= 2.0;
      color.b *= 0.5;
    }
    
    // Random digital glitch blocks
    if(random(floor(deformedUv * 10.0) + floor(time * 2.0)) > 0.995) {
      vec2 blockUv = floor(deformedUv * 10.0) / 10.0;
      color = vec3(random(blockUv + time));
    }
    
    // Extreme vignette with distortion
    vec2 vignetteUv = deformedUv * 2.0 - 1.0;
    float vignette = 1.0 - dot(vignetteUv, vignetteUv) * (1.0 + sin(time * 3.0) * 0.5);
    color *= vignette;
    
    // Pulsing brightness with interference
    float pulse = sin(time * 8.0) * 0.2 + 0.8;
    color *= pulse * (1.0 + interference * 0.2);
    
    // Random color channel swapping
    if(random(vec2(time)) > 0.97) {
      color = color.brg;
    }
    
    gl_FragColor = vec4(color, 0.95);
  }
`;

const TVDistortionEffect = () => {
  const canvasRef = useRef(null);
  const rafRef = useRef(null);
  
  useEffect(() => {
    const canvas = canvasRef.current;
    const gl = canvas.getContext('webgl', {
      alpha: true,
      antialias: false,
      depth: false,
      preserveDrawingBuffer: false
    });
    
    if (!gl) {
      console.error('WebGL not supported');
      return;
    }

    // Create shader program
    const program = gl.createProgram();
    
    // Vertex shader
    const vertShader = gl.createShader(gl.VERTEX_SHADER);
    gl.shaderSource(vertShader, vertexShader);
    gl.compileShader(vertShader);
    gl.attachShader(program, vertShader);
    
    // Fragment shader
    const fragShader = gl.createShader(gl.FRAGMENT_SHADER);
    gl.shaderSource(fragShader, fragmentShader);
    gl.compileShader(fragShader);
    gl.attachShader(program, fragShader);
    
    gl.linkProgram(program);
    gl.useProgram(program);
    
    // Create geometry
    const vertices = new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]);
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
    
    // Set up attributes and uniforms
    const position = gl.getAttribLocation(program, 'position');
    gl.enableVertexAttribArray(position);
    gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
    
    const timeLocation = gl.getUniformLocation(program, 'time');
    const resolutionLocation = gl.getUniformLocation(program, 'resolution');
    
    // Animation loop
    let startTime = Date.now();
    
    const animate = () => {
      const time = (Date.now() - startTime) * 0.001;
      gl.uniform1f(timeLocation, time);
      gl.uniform2f(resolutionLocation, canvas.width, canvas.height);
      
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      rafRef.current = requestAnimationFrame(animate);
    };
    
    const handleResize = () => {
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
      gl.viewport(0, 0, canvas.width, canvas.height);
    };
    
    window.addEventListener('resize', handleResize);
    handleResize();
    animate();
    
    return () => {
      window.removeEventListener('resize', handleResize);
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
      }
      gl.deleteProgram(program);
      gl.deleteShader(vertShader);
      gl.deleteShader(fragShader);
      gl.deleteBuffer(buffer);
    };
  }, []);
  
  return (
    <canvas
      ref={canvasRef}
      className="fixed top-0 left-0 w-full h-full pointer-events-none"
      style={{ 
        mixBlendMode: 'screen',
        opacity: 0.95
      }}
    />
  );
};

export default TVDistortionEffect;
