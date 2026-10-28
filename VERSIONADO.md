# Versiones de Nebula Launcher

Nebula usa el formato **MAYOR.MENOR.PARCHE**, también conocido como SemVer:

```
0.2.0
│ │ └── parche: cambios pequeños
│ └──── menor: cambios relevantes
└────── mayor: cambios grandes o incompatibles
```

## Regla rápida

| Tipo | Cuándo usarlo | Ejemplo |
|---|---|---|
| **Parche** | Arreglos, textos, iconos, ajustes visuales, correcciones de un fallo, cambios internos que no añaden una función | `1.10.1` → `1.10.2` |
| **Menor** | Una función nueva o un cambio que el usuario nota, pero que no rompe lo anterior | `1.10.2` → `1.11.0` |
| **Mayor** | Un rediseño grande, un cambio incompatible o un hito importante | `1.11.1` → `2.0.0` |

Si dudas entre parche y menor, pregúntate: **¿el usuario puede hacer algo nuevo?**
Si la respuesta es sí, es menor. Si solo se corrigió o mejoró algo que ya existía,
es parche.

## Cambios muy pequeños

No hace falta publicar una versión por cada commit. Los cambios diminutos se
agrupan en un solo parche:

- Varios textos, iconos o retoques visuales → un único `1.10.2`.
- Varias correcciones pequeñas → `1.10.3`.
- Si un release falla y hay que corregirlo, se publica el siguiente parche
  (`1.10.4`); **nunca se reutiliza** una versión ya publicada.

Así se evita llenar el historial de releases por cambios que no aportan nada
por separado.

## Ejemplos con la versión actual

La versión publicada ahora es **0.2.0**:

| Cambio | Nueva versión | Comando |
|---|---|---|
| Un texto, un icono o un arreglo pequeño | `0.2.1` | `./scripts/bump-version.sh` |
| Otra corrección pequeña | `0.2.2` | `./scripts/bump-version.sh` |
| Una función nueva o un cambio relevante | `0.3.0` | `./scripts/bump-version.sh minor` |
| Un arreglo pequeño después de `0.3.0` | `0.3.1` | `./scripts/bump-version.sh` |
| Un rediseño o cambio grande | `1.0.0` | `./scripts/bump-version.sh major` |
| Una función relevante dentro de la serie 2 | `2.0.0` → `2.1.0` | `./scripts/bump-version.sh minor` |
| Fijar una versión exacta | `2.1.0` | `./scripts/bump-version.sh set 2.1.0` |

La serie `0.x` se usa mientras el launcher sigue en desarrollo. Cuando se
considere estable, el salto a `1.0.0` se hace una sola vez con
`./scripts/bump-version.sh major` (o `set 1.0.0`).

## Cómo subir la versión

El script actualiza todos los archivos donde vive la versión:

```bash
cd "/home/dns10011/Escritorio/laucher etc/nebula-launcher-fase7-eufonia"

./scripts/bump-version.sh              # parche (opción por defecto)
./scripts/bump-version.sh patch        # parche explícito
./scripts/bump-version.sh minor        # cambio relevante
./scripts/bump-version.sh major        # cambio grande
./scripts/bump-version.sh set 2.1.0    # versión exacta, siempre mayor
./scripts/bump-version.sh 2.1.0        # atajo equivalente a set
./scripts/bump-version.sh --help       # ayuda
```

Actualiza:

- `package.json`
- `package-lock.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `Cargo.toml` (versión del workspace)

`src-tauri/Cargo.lock` se actualiza solo al compilar.

## Reglas de publicación

1. El tag del release debe ser `v` + la versión: `v0.2.1`.
2. `latest.json` debe tener exactamente la misma versión.
3. Los artefactos deben ser los firmados de esa versión.
4. No se reutiliza un tag ni una versión ya publicada.
5. Si un release sale mal, se corrige y se publica el siguiente parche.

Para el proceso completo, ver `INSTRUCCIONES_ACTUALIZACION.txt` y
`INSTRUCCIONES_WINDOWS.txt`.
