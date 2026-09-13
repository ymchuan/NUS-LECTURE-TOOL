import { describe, expect, it } from "vitest";
import { downsampleToPcm16, pcmToBase64 } from "./audio";

describe("downsampleToPcm16", () => {
  it("converts a 100 ms 48 kHz chunk to 16 kHz mono PCM", () => {
    const input = new Float32Array(4_800).fill(0.5);
    const output = downsampleToPcm16(input, 48_000);

    expect(output).toHaveLength(3_200);
    expect(new DataView(output.buffer).getInt16(0, true)).toBe(16_384);
  });

  it("clamps samples and writes little-endian signed values", () => {
    const output = downsampleToPcm16(new Float32Array([2, -2]), 16_000);
    const view = new DataView(output.buffer);

    expect(view.getInt16(0, true)).toBe(32_767);
    expect(view.getInt16(2, true)).toBe(-32_768);
  });
});

describe("pcmToBase64", () => {
  it("encodes binary PCM without expanding it to a JSON number array", () => {
    expect(pcmToBase64(new Uint8Array([0, 127, 128, 255]))).toBe("AH+A/w==");
  });
});
