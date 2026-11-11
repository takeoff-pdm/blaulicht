// Core ML diagnostics for the Whisper models: where each operation runs, and how fast each model is.
//
//   diag plan  <model.mlmodelc> <ane|gpu|cpu|all>        per-op device placement (MLComputePlan)
//   diag bench <model.mlmodelc> <ane|gpu|cpu|all> [n]    load time, first and median prediction on zero inputs
//   diag devices
//   DIAG_FUNCTION=<name> picks one function of a multifunction model.
// Build: env -i HOME=$HOME PATH=/usr/bin:/bin DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
//          xcrun swiftc -O -parse-as-library -target arm64-apple-macosx15.0 diag.swift -o diag

import CoreML
import Foundation

func units(_ s: String) -> MLComputeUnits {
    switch s {
    case "gpu": return .cpuAndGPU
    case "cpu": return .cpuOnly
    case "all": return .all
    default: return .cpuAndNeuralEngine
    }
}

func name(_ d: MLComputeDevice) -> String {
    switch d {
    case .cpu: return "cpu"
    case .gpu: return "gpu"
    case .neuralEngine: return "ane"
    @unknown default: return "?"
    }
}

func plan(_ url: URL, _ cfg: MLModelConfiguration) async throws {
    let t = Date()
    let plan = try await MLComputePlan.load(contentsOf: url, configuration: cfg)
    print(String(format: "plan loaded in %.1f s", Date().timeIntervalSince(t)))
    var byDev: [String: (n: Int, cost: Double)] = [:]
    var off: [(op: String, out: String, cost: Double, supported: String)] = []
    func walk(_ block: MLModelStructure.Program.Block) {
        for op in block.operations {
            for b in op.blocks { walk(b) }
            guard op.operatorName != "const", let usage = plan.deviceUsage(for: op) else { continue }
            let dev = name(usage.preferred)
            let cost = plan.estimatedCost(of: op)?.weight ?? 0
            byDev[dev, default: (0, 0)].n += 1
            byDev[dev]!.cost += cost
            if dev != "ane" {
                off.append((op.operatorName, op.outputs.map { $0.name }.joined(separator: ","), cost,
                            usage.supported.map(name).joined(separator: ",")))
            }
        }
    }
    func structure(_ s: MLModelStructure, _ label: String) {
        switch s {
        case .program(let prog):
            for (fname, f) in prog.functions {
                if let want = ProcessInfo.processInfo.environment["DIAG_FUNCTION"], want != fname { continue }
                print("\(label) function \(fname)")
                walk(f.block)
            }
        case .pipeline(let pipe):
            for (i, sub) in pipe.subModels.enumerated() { structure(sub, "\(label)/\(i)") }
        default:
            print("\(label): not an ML program or pipeline")
        }
    }
    structure(plan.modelStructure, "model")
    for (dev, v) in byDev.sorted(by: { $0.key < $1.key }) {
        print(String(format: "  %@: %d ops, %.1f%% of estimated cost", dev, v.n, v.cost * 100))
    }
    print("  ops not on the Neural Engine (op, output, cost share, devices that support it):")
    for o in off.sorted(by: { $0.cost > $1.cost }) {
        print(String(format: "    %-22@ %-40@ %6.3f%%  [%@]", o.op, o.out, o.cost * 100, o.supported))
    }
}

func bench(_ url: URL, _ cfg: MLModelConfiguration, _ n: Int) throws {
    let tl = Date()
    let model = try MLModel(contentsOf: url, configuration: cfg)
    let load = Date().timeIntervalSince(tl)
    var feats: [String: MLFeatureValue] = [:]
    for (k, d) in model.modelDescription.inputDescriptionsByName {
        guard let c = d.multiArrayConstraint else { continue }
        let arr = try MLMultiArray(shape: c.shape, dataType: c.dataType)
        arr.withUnsafeMutableBytes { p, _ in _ = memset(p.baseAddress!, 0, p.count) }
        feats[k] = MLFeatureValue(multiArray: arr)
    }
    let input = try MLDictionaryFeatureProvider(dictionary: feats)
    let state: MLState? = model.modelDescription.stateDescriptionsByName.isEmpty ? nil : model.makeState()
    var times: [Double] = []
    for _ in 0..<n {
        let t = Date()
        if let state { _ = try model.prediction(from: input, using: state) } else { _ = try model.prediction(from: input) }
        times.append(Date().timeIntervalSince(t))
    }
    let rest = times.dropFirst().sorted()
    let med = rest.isEmpty ? times[0] : rest[rest.count / 2]
    print(String(format: "%@ on %@: load %.2f s, first %.1f ms, median %.1f ms (n=%d)",
                 url.deletingPathExtension().lastPathComponent, CommandLine.arguments[3], load, times[0] * 1000, med * 1000, n))
}

@main
struct Diag {
    static func main() async throws {
        let a = CommandLine.arguments
        if a.count == 2 && a[1] == "devices" {
            for d in MLComputeDevice.allComputeDevices {
                if case .neuralEngine(let ne) = d { print("ane, \(ne.totalCoreCount) cores") } else { print(name(d)) }
            }
            return
        }
        guard a.count >= 4 else { print("usage: diag plan|bench <model.mlmodelc> <ane|gpu|cpu|all> [n]"); exit(2) }
        let cfg = MLModelConfiguration()
        cfg.computeUnits = units(a[3])
        if let f = ProcessInfo.processInfo.environment["DIAG_FUNCTION"] { cfg.functionName = f }
        let url = URL(fileURLWithPath: a[2])
        if a[1] == "plan" { try await plan(url, cfg) } else { try bench(url, cfg, a.count > 4 ? Int(a[4])! : 10) }
    }
}
