// Offscreen WKWebView snapshot: swift snap.swift <url> <cssWidth> <cssHeight> <out.png> [delaySeconds]
// The PNG has the backing scale of the main display (2x on Retina Macs).
import Cocoa
import WebKit

let args = CommandLine.arguments
guard args.count >= 5, let url = URL(string: args[1]),
      let w = Double(args[2]), let h = Double(args[3]) else {
    FileHandle.standardError.write("usage: snap <url> <width> <height> <out.png> [delay]\n".data(using: .utf8)!)
    exit(2)
}
let out = args[4]
let delay = args.count > 5 ? (Double(args[5]) ?? 1.5) : 1.5

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)

let config = WKWebViewConfiguration()
let webView = WKWebView(frame: NSRect(x: 0, y: 0, width: w, height: h), configuration: config)

// An (unshown) window gives the view a backing store and a run loop target.
let window = NSWindow(contentRect: webView.frame, styleMask: [.borderless], backing: .buffered, defer: false)
window.contentView = webView

final class Delegate: NSObject, WKNavigationDelegate {
    var done = false
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            let cfg = WKSnapshotConfiguration()
            cfg.rect = webView.bounds
            webView.takeSnapshot(with: cfg) { image, error in
                defer { self.done = true }
                guard let image = image, let tiff = image.tiffRepresentation,
                      let rep = NSBitmapImageRep(data: tiff),
                      let png = rep.representation(using: .png, properties: [:]) else {
                    FileHandle.standardError.write("snapshot failed: \(String(describing: error))\n".data(using: .utf8)!)
                    exit(1)
                }
                do {
                    try png.write(to: URL(fileURLWithPath: out))
                    print("wrote \(out) \(rep.pixelsWide)x\(rep.pixelsHigh)")
                } catch {
                    FileHandle.standardError.write("write failed: \(error)\n".data(using: .utf8)!)
                    exit(1)
                }
            }
        }
    }
    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        FileHandle.standardError.write("load failed: \(error)\n".data(using: .utf8)!)
        exit(1)
    }
}

let delegate = Delegate()
webView.navigationDelegate = delegate
webView.load(URLRequest(url: url))

let deadline = Date().addingTimeInterval(30)
while !delegate.done && Date() < deadline {
    RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
}
if !delegate.done {
    FileHandle.standardError.write("timeout\n".data(using: .utf8)!)
    exit(1)
}
