// CLOISON Mobile — iOS : contenu principal (WebView de chat + sheet réglages).

import SwiftUI

struct ContentView: View {
    @State private var showSettings = false

    var body: some View {
        WebViewContainer(onOpenSettings: { showSettings = true })
            .ignoresSafeArea(edges: .bottom)
            .sheet(isPresented: $showSettings) {
                SettingsView()
            }
    }
}
