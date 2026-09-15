import * as THREE from "three"
import { OrbitControls } from "three/addons/controls/OrbitControls.js"
import type { GLTF } from "three/addons/loaders/GLTFLoader.js"

const MAX_PIXEL_RATIO = 1.5
const MAX_VERTICES = 5_000_000
const BASE_FIELD_OF_VIEW = 45

export function disposeModel(root: THREE.Object3D) {
  const textures = new Set<THREE.Texture>()
  const materials = new Set<THREE.Material>()
  root.traverse((node) => {
    const mesh = node as THREE.Mesh
    mesh.geometry?.dispose()
    if (mesh.material)
      for (const material of Array.isArray(mesh.material)
        ? mesh.material
        : [mesh.material])
        materials.add(material)
  })
  for (const material of materials) {
    for (const value of Object.values(material))
      if (value instanceof THREE.Texture) textures.add(value)
    material.dispose()
  }
  for (const texture of textures) {
    if (
      typeof ImageBitmap !== "undefined" &&
      texture.image instanceof ImageBitmap
    )
      texture.image.close()
    texture.dispose()
  }
}

export function createModelScene(host: HTMLElement) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
  renderer.setPixelRatio(
    Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO)
  )
  renderer.setClearColor(0xe5e7eb)
  host.appendChild(renderer.domElement)
  const scene = new THREE.Scene()
  const camera = new THREE.PerspectiveCamera(BASE_FIELD_OF_VIEW, 1, 0.01, 1000)
  camera.position.set(3, 2, 3)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.enableDamping = false
  scene.add(new THREE.HemisphereLight(0xffffff, 0x667766, 3))
  const light = new THREE.DirectionalLight(0xffffff, 3)
  light.position.set(4, 7, 5)
  scene.add(light)
  return new ModelScene({ host, renderer, scene, camera, controls })
}

interface SceneContext {
  host: HTMLElement
  renderer: THREE.WebGLRenderer
  scene: THREE.Scene
  camera: THREE.PerspectiveCamera
  controls: OrbitControls
}

class ModelScene {
  private model?: GLTF
  private mixer?: THREE.AnimationMixer
  private disposed = false
  private lastTime = 0
  private observer: ResizeObserver
  constructor(private context: SceneContext) {
    this.observer = new ResizeObserver(this.resize)
    this.observer.observe(context.host)
    context.controls.addEventListener("change", this.draw)
  }
  private draw = () => {
    const { renderer, scene, camera } = this.context
    if (!this.disposed) renderer.render(scene, camera)
  }
  private resize = () => {
    const { host, renderer, camera } = this.context
    if (this.disposed || !host.clientWidth || !host.clientHeight) return
    renderer.setSize(host.clientWidth, host.clientHeight)
    camera.aspect = host.clientWidth / host.clientHeight
    camera.fov = THREE.MathUtils.radToDeg(
      2 *
        Math.atan(
          Math.tan(THREE.MathUtils.degToRad(BASE_FIELD_OF_VIEW / 2)) /
            Math.min(1, camera.aspect)
        )
    )
    camera.updateProjectionMatrix()
    this.draw()
  }
  setModel(value: GLTF) {
    if (this.disposed) {
      for (const root of value.scenes) disposeModel(root)
      return
    }
    this.model = value
    checkGeometry(value.scene)
    const { scene, camera, controls } = this.context
    scene.add(value.scene)
    fitModel(value.scene, camera, controls)
    controls.saveState()
    this.mixer = new THREE.AnimationMixer(value.scene)
    for (const clip of value.animations) this.mixer.clipAction(clip).play()
    this.resize()
  }
  zoom(value: number) {
    this.context.camera.zoom = value
    this.context.camera.updateProjectionMatrix()
    this.draw()
  }
  reset() {
    this.context.controls.reset()
    this.draw()
  }
  play(enabled: boolean) {
    this.lastTime = 0
    this.context.renderer.setAnimationLoop(
      enabled
        ? (time) => {
            if (this.lastTime)
              this.mixer?.update(Math.min((time - this.lastTime) / 1000, 0.1))
            this.lastTime = time
            this.draw()
          }
        : null
    )
  }
  dispose() {
    if (this.disposed) return
    this.disposed = true
    const { renderer, controls, scene } = this.context
    renderer.setAnimationLoop(null)
    this.observer.disconnect()
    controls.removeEventListener("change", this.draw)
    controls.dispose()
    this.mixer?.stopAllAction()
    if (this.model) {
      this.mixer?.uncacheRoot(this.model.scene)
      for (const root of this.model.scenes) disposeModel(root)
    }
    scene.clear()
    renderer.dispose()
    renderer.forceContextLoss()
    renderer.domElement.width = 0
    renderer.domElement.height = 0
    renderer.domElement.remove()
    this.model = undefined
    this.mixer = undefined
  }
}

function checkGeometry(root: THREE.Object3D) {
  let vertices = 0
  root.traverse((node) => {
    vertices += (node as THREE.Mesh).geometry?.attributes.position?.count ?? 0
  })
  if (vertices > MAX_VERTICES) {
    throw new Error("Model exceeds preview geometry limit")
  }
}

function fitModel(
  root: THREE.Object3D,
  camera: THREE.PerspectiveCamera,
  controls: OrbitControls
) {
  const bounds = new THREE.Box3().setFromObject(root)
  if (bounds.isEmpty()) throw new Error("Model has no visible geometry")
  const sphere = bounds.getBoundingSphere(new THREE.Sphere())
  const radius = Math.max(sphere.radius, 0.001)
  const distance =
    (radius / Math.sin(THREE.MathUtils.degToRad(BASE_FIELD_OF_VIEW / 2))) * 1.3
  camera.position
    .copy(sphere.center)
    .add(new THREE.Vector3(1, 0.6, 1).normalize().multiplyScalar(distance))
  camera.near = radius / 1000
  camera.far = radius * 100
  camera.updateProjectionMatrix()
  controls.target.copy(sphere.center)
  controls.minDistance = radius / 100
  controls.maxDistance = radius * 30
  controls.update()
}
