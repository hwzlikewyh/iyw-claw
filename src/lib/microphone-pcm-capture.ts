export interface MicrophonePcmCapture {
  stop: () => Promise<void>
}

type AudioContextConstructor = typeof AudioContext

interface CaptureNodes {
  stream: MediaStream
  context: AudioContext
  source: MediaStreamAudioSourceNode
  processor: AudioWorkletNode
  silentOutput: GainNode
  setFlushResolver: (resolve: () => void) => void
}

function audioContextConstructor(): AudioContextConstructor | null {
  const webkit = window as typeof window & {
    webkitAudioContext?: AudioContextConstructor
  }
  return window.AudioContext ?? webkit.webkitAudioContext ?? null
}

function requireMicrophoneSupport(): AudioContextConstructor {
  const AudioContextImpl = audioContextConstructor()
  if (!navigator.mediaDevices?.getUserMedia || !AudioContextImpl) {
    throw new DOMException(
      "Microphone input is unavailable",
      "NotSupportedError"
    )
  }
  return AudioContextImpl
}

export async function startMicrophonePcmCapture(
  onChunk: (chunk: Uint8Array) => void
): Promise<MicrophonePcmCapture> {
  const AudioContextImpl = requireMicrophoneSupport()
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      channelCount: 1,
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    },
    video: false,
  })
  let context: AudioContext | null = null
  try {
    context = new AudioContextImpl()
    await context.audioWorklet.addModule("/realtime-pcm-worklet.js")
    const source = context.createMediaStreamSource(stream)
    const processor = new AudioWorkletNode(context, "realtime-pcm-processor", {
      channelCount: 1,
      outputChannelCount: [1],
    })
    const silentOutput = context.createGain()
    silentOutput.gain.value = 0
    let flushResolver: (() => void) | null = null
    processor.port.onmessage = (
      event: MessageEvent<ArrayBuffer | { type: string }>
    ) => {
      if (event.data instanceof ArrayBuffer) {
        if (event.data.byteLength > 0) onChunk(new Uint8Array(event.data))
        return
      }
      if (event.data.type === "flushed") flushResolver?.()
    }
    source.connect(processor)
    processor.connect(silentOutput)
    silentOutput.connect(context.destination)
    await context.resume()
    return createCapture({
      stream,
      context,
      source,
      processor,
      silentOutput,
      setFlushResolver: (resolve) => {
        flushResolver = resolve
      },
    })
  } catch (error) {
    stream.getTracks().forEach((track) => track.stop())
    if (context) await context.close().catch(() => {})
    throw error
  }
}

function createCapture(nodes: CaptureNodes): MicrophonePcmCapture {
  const { stream, context, source, processor, silentOutput, setFlushResolver } =
    nodes
  let stopped = false
  return {
    async stop() {
      if (stopped) return
      stopped = true
      await new Promise<void>((resolve) => {
        const fallback = window.setTimeout(resolve, 250)
        setFlushResolver(() => {
          window.clearTimeout(fallback)
          resolve()
        })
        processor.port.postMessage({ type: "flush" })
      })
      processor.port.onmessage = null
      stream.getTracks().forEach((track) => track.stop())
      source.disconnect()
      processor.disconnect()
      silentOutput.disconnect()
      await context.close().catch(() => {})
    },
  }
}
