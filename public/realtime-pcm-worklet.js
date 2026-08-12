const TARGET_SAMPLE_RATE = 16000
const CHUNK_SAMPLES = 1600

class RealtimePcmProcessor extends AudioWorkletProcessor {
  constructor() {
    super()
    this.ratio = sampleRate / TARGET_SAMPLE_RATE
    this.carry = new Float32Array(0)
    this.position = 0
    this.chunk = new Int16Array(CHUNK_SAMPLES)
    this.chunkLength = 0
    this.port.onmessage = (event) => {
      if (event.data?.type !== "flush") return
      if (this.chunkLength > 0) {
        const tail = this.chunk.slice(0, this.chunkLength)
        this.chunkLength = 0
        this.port.postMessage(tail.buffer, [tail.buffer])
      }
      this.port.postMessage({ type: "flushed" })
    }
  }

  process(inputs, outputs) {
    const input = inputs[0]?.[0]
    const output = outputs[0]?.[0]
    if (output) output.fill(0)
    if (input?.length) this.resample(input)
    return true
  }

  resample(input) {
    const samples = new Float32Array(this.carry.length + input.length)
    samples.set(this.carry)
    samples.set(input, this.carry.length)
    while (this.position + 1 < samples.length) {
      const left = Math.floor(this.position)
      const fraction = this.position - left
      const value =
        samples[left] + (samples[left + 1] - samples[left]) * fraction
      this.pushSample(value)
      this.position += this.ratio
    }
    const consumed = Math.min(
      Math.floor(this.position),
      Math.max(0, samples.length - 1)
    )
    this.carry = samples.slice(consumed)
    this.position -= consumed
  }

  pushSample(value) {
    const clamped = Math.max(-1, Math.min(1, value))
    this.chunk[this.chunkLength] =
      clamped < 0 ? Math.round(clamped * 32768) : Math.round(clamped * 32767)
    this.chunkLength += 1
    if (this.chunkLength !== CHUNK_SAMPLES) return
    const complete = this.chunk
    this.chunk = new Int16Array(CHUNK_SAMPLES)
    this.chunkLength = 0
    this.port.postMessage(complete.buffer, [complete.buffer])
  }
}

registerProcessor("realtime-pcm-processor", RealtimePcmProcessor)
