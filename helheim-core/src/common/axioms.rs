use serde::{Deserialize, Serialize};

/// Jan's Regels (The 10 Axioms of MOM)
/// These rules govern the decision-making process of the Helheim/PEPAI system.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Axiom {
    /// 1. Wat wil ik *echt*? (Core Goal)
    Intentie,
    /// 2. Hoe kan het *niet*? (Via Negativa)
    Inversie,
    /// 3. Kleine, werkende stappen. (Iterative)
    Progressie,
    /// 4. Als A en B botsen, klopt C niet. (Logic Check)
    Contradictie,
    /// 5. Alles is relatief aan de omgeving. (Environmental Awareness)
    Context,
    /// 6. Begin opnieuw met nieuwe kennis. (Recursion)
    ReEntry,
    /// 7. Filter alles wat geen signaal is. (Noise Reduction)
    Ruis,
    /// 8. Zoom in/uit om het probleem te zien. (Perspective Shift)
    Schaal,
    /// 9. Bekijk het van de andere kant. (Empathy/Devil's Advocate)
    Perspectief,
    /// 10. Stop als het fout gaat. (Circuit Breaker)
    Interruptie,
}

impl Axiom {
    pub fn description(&self) -> &'static str {
        match self {
            Axiom::Intentie => "Definieer het echte doel, niet de methode.",
            Axiom::Inversie => "Draai het probleem om; wat moet ik vermijden?",
            Axiom::Progressie => "Zet een stap die werkt, hoe klein ook.",
            Axiom::Contradictie => "Detecteer paradoxen; ze wijzen naar een fout in de aannames.",
            Axiom::Context => "Houd rekening met de huidige staat en omgeving.",
            Axiom::ReEntry => "Gebruik de uitkomst van de vorige stap als nieuwe input.",
            Axiom::Ruis => "Negeer irrelevante data.",
            Axiom::Schaal => "Verander het abstractieniveau.",
            Axiom::Perspectief => "Simuleer een andere actor.",
            Axiom::Interruptie => "Breek de loop bij kritieke fouten.",
        }
    }
}

/// The Soul Struct: Holds the active state of the Axioms.
pub struct Soul {
    pub active_axiom: Axiom,
}

impl Soul {
    pub fn new() -> Self {
        Self {
            active_axiom: Axiom::Intentie, // Start always with Intent
        }
    }

    pub fn consult(&self) -> &'static str {
        self.active_axiom.description()
    }
}
