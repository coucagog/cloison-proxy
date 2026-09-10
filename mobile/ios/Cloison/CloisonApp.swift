// CLOISON Mobile — iOS v1 (app WebView + moteur WASM)
//
// Miroir de l'app Android (mobile/android/) : pseudonymisation IN-APP via le
// module WASM @cloison/core, coffre in-memory, chat + endpoint LLM
// configurable. Le corps HTTP envoyé au fournisseur est TOKENISÉ côté JS —
// le fournisseur ne voit jamais la PII en clair.
//
// Invariants (charte) :
//   - zéro PII persistée : clé locataire aléatoire en mémoire, coffre
//     in-memory, rien sur disque (hors la clé amont dans UserDefaults) ;
//   - zéro secret embarqué : aucun secret dans le code/l'app ;
//   - restauration bornée : registre de la requête + MAC (I3) ;
//   - honnêteté (§11) : poste compromis non protégé, quasi-ids signalés.

import SwiftUI

@main
struct CloisonApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
