import GrimoireCore
import SwiftUI

struct MainWindowView: View {
    let player: PlayerController
    let windows: WindowManager

    var body: some View {
        VStack(spacing: 8) {
            TitleBar(title: "GRIMOIRE", onMinimize: windows.minimize, onClose: windows.quit)
            HStack(alignment: .top, spacing: 8) {
                LCDDisplay(player: player)
                VStack(spacing: 7) {
                    Marquee(text: player.marqueeText)
                    InfoReadout(info: player.fileInfo)
                    HStack(spacing: 6) {
                        Image(systemName: "speaker.wave.2.fill")
                            .font(.system(size: 9))
                            .foregroundStyle(Theme.textDim)
                        RetroSlider(value: player.volume) { player.volume = $0 }
                            .help("Volume")
                    }
                }
            }
            SeekBar(player: player)
            HStack(spacing: 3) {
                TransportButton(symbol: "backward.end.fill", help: "Previous") { player.previous() }
                TransportButton(symbol: "play.fill", help: "Play") { player.play() }
                TransportButton(symbol: "pause.fill", help: "Pause") { player.pause() }
                TransportButton(symbol: "stop.fill", help: "Stop") { player.stop() }
                TransportButton(symbol: "forward.end.fill", help: "Next") { player.next() }
                TransportButton(symbol: "eject.fill", help: "Open files…") { player.openFiles() }
                    .padding(.leading, 6)
                Spacer(minLength: 8)
                LEDToggle(label: "SHUF", isOn: player.shuffle) { player.toggleShuffle() }
                LEDToggle(label: repeatLabel, isOn: player.repeatMode != .off) { player.cycleRepeat() }
                LEDToggle(label: "PL", isOn: windows.isPlaylistVisible) { windows.togglePlaylist() }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(width: 520)
        .background(WindowChrome())
        .dropDestination(for: URL.self) { urls, _ in
            player.add(urls, playFirst: true)
            return true
        }
    }

    private var repeatLabel: String {
        player.repeatMode == .one ? "REP 1" : "REP"
    }
}

/// Big time readout with "ghost" segments behind it, with the visualizer underneath.
private struct LCDDisplay: View {
    let player: PlayerController

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .center, spacing: 6) {
                Image(systemName: stateSymbol)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Theme.lcdText)
                    .frame(width: 12)
                TimelineView(.periodic(from: .now, by: 0.5)) { context in
                    let blinkOff = player.state == .paused
                        && Int(context.date.timeIntervalSinceReferenceDate * 2) % 2 == 0
                    ZStack(alignment: .leading) {
                        Text("-88:88").foregroundStyle(Theme.lcdGhost)
                        Text(timeText)
                            .foregroundStyle(Theme.lcdText)
                            .shadow(color: Theme.lcdText.opacity(0.6), radius: 4)
                            .opacity(blinkOff ? 0 : 1)
                    }
                    .font(Theme.lcdFont(30))
                }
                .contentShape(Rectangle())
                .onTapGesture { player.showRemaining.toggle() }
                .help("Click to toggle elapsed / remaining")
            }
            VisualizerView(player: player)
        }
        .padding(8)
        .frame(width: 196, alignment: .topLeading)
        .lcdPanel()
    }

    private var stateSymbol: String {
        switch player.state {
        case .playing: "play.fill"
        case .paused: "pause.fill"
        case .stopped: "stop.fill"
        }
    }

    private var timeText: String {
        guard player.state != .stopped else { return " 00:00" }
        if player.showRemaining {
            return "-" + TimeFormat.lcd(max(0, player.duration - player.currentTime))
        }
        return " " + TimeFormat.lcd(player.currentTime)
    }
}

/// Scrolling title readout. Only animates when the text doesn't fit.
private struct Marquee: View {
    let text: String
    private let visibleCharacters = 36
    private let step: TimeInterval = 0.2

    var body: some View {
        Group {
            if text.count > visibleCharacters {
                TimelineView(.periodic(from: .now, by: step)) { context in
                    label(window(at: context.date))
                }
            } else {
                label(text)
            }
        }
        .padding(.horizontal, 6)
        .frame(height: 22)
        .lcdPanel()
    }

    private func label(_ string: String) -> some View {
        Text(string)
            .font(Theme.lcdFont(12))
            .foregroundStyle(Theme.lcdText)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func window(at date: Date) -> String {
        let loop = Array(text + "   ✦   ")
        let start = Int(date.timeIntervalSinceReferenceDate / step) % loop.count
        return String((0..<visibleCharacters).map { loop[(start + $0) % loop.count] })
    }
}

/// Format / bitrate / sample rate / channels.
private struct InfoReadout: View {
    let info: PlayerEngine.FileInfo?

    var body: some View {
        HStack(spacing: 5) {
            cell(info?.format ?? "---", label: nil, width: 44)
            cell(info?.bitrate.map(String.init) ?? "---", label: "KBPS", width: 72)
            cell(info.map { String(format: "%g", $0.sampleRate / 1000) } ?? "--", label: "KHZ", width: 76)
            Spacer(minLength: 0)
            Text(channelText)
                .font(Theme.labelFont(8))
                .tracking(1)
                .foregroundStyle(info == nil ? Theme.textDim : Theme.lcdText)
        }
    }

    private var channelText: String {
        guard let info else { return "" }
        switch info.channels {
        case 1: return "MONO"
        case 2: return "STEREO"
        default: return "\(info.channels) CH"
        }
    }

    private func cell(_ value: String, label: String?, width: CGFloat) -> some View {
        HStack(spacing: 3) {
            Text(value)
                .font(Theme.lcdFont(10))
                .foregroundStyle(Theme.lcdText)
                .fixedSize()
                .padding(.horizontal, 4)
                .frame(height: 16)
                .frame(minWidth: 34)
                .lcdPanel(cornerRadius: 2)
            if let label {
                Text(label).font(Theme.labelFont(7)).foregroundStyle(Theme.textDim)
            }
        }
        .frame(width: width, alignment: .leading)
    }
}

private struct SeekBar: View {
    let player: PlayerController

    var body: some View {
        RetroSlider(
            value: player.duration > 0 ? player.currentTime / player.duration : 0,
            live: false,
            enabled: player.state != .stopped && player.duration > 0
        ) { fraction in
            player.seek(to: fraction * player.duration)
        }
        .help("Seek")
    }
}
