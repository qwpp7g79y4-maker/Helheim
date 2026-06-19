# Helheim v2.0 - Cheat Sheet

## 🇳🇱 Syntax & Concepten (Dutch) / 🇬🇧 Syntax & Concepts (English)

### Variabelen & Printen / Variables & Printing
```helheim
// Variabele toewijzen (Assign variable)
zet x = 10;
zet tekst = "Hallo Wereld!";

// Printen naar console (Print to console)
print x;
print "De tekst is: " + tekst;
```

### Modellen (Structs) / Models (Structs)
```helheim
model Persoon {
    naam,
    leeftijd
}

// Initialisatie (Initialization)
zet jan = nieuw Persoon("Jan", 30);
```

### Functies / Functions
```helheim
functie tel_op(a, b) {
    geef_terug a + b;
}

zet resultaat = roep_aan tel_op(5, 10);
print resultaat;
```

### Control Flow (If & Loop)
```helheim
als x > 5 dan {
    print "Groot!";
} anders {
    print "Klein!";
}

zet i = 0;
zolang i < 5 {
    print i;
    zet i = i + 1;
}

voor elke item in [1, 2, 3] {
    print item;
}
```

### Effect Handlers (Algebraic Effects)
Scheid wat je wilt doen van hóe het uitgevoerd wordt.
*Separate what you want to do from how it is executed.*

```helheim
// Effect Definitie
effect Groet {
    zeg_hallo
}

// Handler
handle Groet {
    zeg_hallo => {
        let naam = arg1;
        print "Hallo vanuit de handler, " + naam;
        geef_terug "Succes";
    }
} in {
    // Perform trigger
    let res = perform Groet.zeg_hallo("Bob");
    print "Resultaat: " + res;
}
```

### Actoren & Concurrency / Actors & Concurrency
```helheim
// Spawn een taak (Spawn a task)
spawn {
    print "Dit draait in een losse tokio task!";
}

// Actor model met Supervisor Strategie (Stop, Restart, Escalate)
perform Actor.spawn("{
    gooi \"Fout!\";
}", "Escalate"); // "Escalate" stuurt de fout naar de mailbox van de parent
spawn "MijnActor" {
    zolang waar {
        ontvang msg binnen 5000 {
            als msg == "stop" dan { stop; }
            print "Bericht ontvangen: " + msg;
        }
    }
}

// Stuur een bericht naar een actor
stuur "MijnActor" "Hallo!";
```

### Imports (Gebruik)
```helheim
// Helheim standard library inladen
gebruik "stdlib/pure/net.hel";

// Functies uit modules worden first-class aangeroepen via de namespace:
let data = roep_aan Net::fetch("http://localhost");

// FFI Shared Library (C/C++) inladen
gebruik "stdlib/lib/libhelheim_math_plugin.so";
```

### Migratie & Continuations / Migration & Continuations
```helheim
// Een script kan midden in de executie pauzeren, zijn geheugen en callstack inpakken,
// en zichzelf over het netwerk sturen naar een andere node (teleportatie).
handle Migratie {
    voor_vertrek => {
        print "We sluiten open resources voordat we weggaan!";
        hervat(""); // Ga door met de migratie
    }
    na_aankomst => {
        print "We zijn aangekomen op de nieuwe node, herstel connecties!";
        hervat(""); // Ga door met de gepauzeerde code
    }
} in {
    // Verplaats de executie naar 192.168.1.10 poort 8080
    perform Swarm::migrate("192.168.1.10", 8080);
    print "Dit wordt uitgevoerd op de NIEUWE node!";
}
```

### Inline Assembly (Bare Metal PTX)
```helheim
// Voer direct hardware-specifieke instructies uit
asm ptx {
    "add.u32 %0, %1, %2;"
} in(a, b) out(c) clobber("memory");
```
