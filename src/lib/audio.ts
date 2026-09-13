export function downsampleToPcm16(
  input: Float32Array,
  inputSampleRate: number,
  outputSampleRate = 16_000,
): Uint8Array {
  if (!input.length || inputSampleRate <= 0 || outputSampleRate <= 0) {
    return new Uint8Array();
  }

  const ratio = Math.max(1, inputSampleRate / outputSampleRate);
  const outputLength = Math.floor(input.length / ratio);
  const output = new Uint8Array(outputLength * 2);
  const view = new DataView(output.buffer);

  for (let index = 0; index < outputLength; index += 1) {
    const start = Math.floor(index * ratio);
    const end = Math.max(start + 1, Math.min(input.length, Math.floor((index + 1) * ratio)));
    let total = 0;
    for (let sourceIndex = start; sourceIndex < end; sourceIndex += 1) {
      total += input[sourceIndex];
    }
    const sample = Math.max(-1, Math.min(1, total / (end - start)));
    const pcm = sample < 0 ? Math.round(sample * 0x8000) : Math.round(sample * 0x7fff);
    view.setInt16(index * 2, pcm, true);
  }

  return output;
}

export function pcmToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let index = 0; index < bytes.length; index += 1) {
    binary += String.fromCharCode(bytes[index]);
  }
  return btoa(binary);
}
