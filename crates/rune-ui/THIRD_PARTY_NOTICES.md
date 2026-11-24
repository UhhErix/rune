# Third-Party Notices & License Audit

`rune-ui` is an original Rust/Rune implementation inspired by the interaction semantics,
accessibility contracts, visual ergonomics, and API patterns of the React UI ecosystem.
All code in `rune-ui` is authored in Rust and WGSL for Rune; no JavaScript, TypeScript,
CSS, or GLSL source files are copied verbatim. Where design or behavioral patterns draw
from permissively licensed reference libraries, attribution is recorded below.

## Verified Reference Library Licenses

Audit date: 2026-11-24 (verified against each upstream GitHub repository `LICENSE` file).

| Library | Repository | SPDX / License | Upstream Copyright | Porting Policy in `rune-ui` |
| --- | --- | --- | --- | --- |
| **HeroUI** (formerly NextUI) | `heroui-inc/heroui` | `Apache-2.0` | HeroUI / NextUI Contributors | Behavioral & visual patterns allowed with attribution (Apache-2.0). |
| **shadcn/ui** | `shadcn-ui/ui` | `MIT` | Copyright (c) 2023 shadcn | Component patterns & proportions allowed with attribution (MIT). |
| **Radix UI Primitives** | `radix-ui/primitives` | `MIT` | Copyright (c) 2022 WorkOS | Accessibility, roving tabindex, focus trap, dismiss & popper semantics allowed with attribution (MIT). |
| **React Aria** | `adobe/react-spectrum` | `Apache-2.0` | Adobe Inc. | Keyboard, calendar, range slider & ARIA interaction semantics allowed with attribution (Apache-2.0). |
| **cmdk** | `pacocoursey/cmdk` | `MIT` | Copyright (c) 2022 Paco Coursey | Command menu fuzzy ranking, group visibility & nested page semantics allowed with attribution (MIT). |
| **Sonner** | `emilkowalski/sonner` | `MIT` | Copyright (c) 2023 Emil Kowalski | Toast stacking, hover expansion, timer pause & swipe-to-dismiss physics allowed with attribution (MIT). |
| **Vaul** | `emilkowalski/vaul` | `MIT` | Copyright (c) 2023 Emil Kowalski | Drawer/sheet snap points, 1:1 drag, velocity fling & background scale semantics allowed with attribution (MIT). |
| **Magic UI** | `magicuidesign/magicui` | `MIT` | Copyright (c) Magic UI | Border beam, marquee, number ticker & grid effect concepts allowed with attribution (MIT). |
| **React Bits** | `DavidHDev/react-bits` | **MIT + Commons Clause v1.0** (`NOASSERTION`) | Copyright (c) 2026 David Haz | **NO CODE PORTED.** The Commons Clause restricts redistributing ported source code as a standalone component library. All effects in `rune-ui::effects` are 100% clean-room original WGSL & Rust implementations written from mathematical and visual first principles only. |
| **Mantine** | `mantinedev/mantine` | `MIT` | Copyright (c) 2021 Vitaly Rtishchev | Input addons, pagination ellipsis, date picker & data display ergonomics allowed with attribution (MIT). |
| **Tremor** | `tremorlabs/tremor` | `Apache-2.0` | Tremor Labs | Dashboard chart framing, KPI donut & tooltip ergonomics allowed with attribution (Apache-2.0). |
| **Recharts** | `recharts/recharts` | `MIT` | Copyright (c) 2015-present recharts | Line, area, bar, donut & sparkline chart composition semantics allowed with attribution (MIT). |
| **Motion** (formerly Framer Motion) | `motiondivision/motion` | `MIT` | Copyright (c) 2024 Motion B.V. | Layout animation, shared-element (`layoutId` -> `El::matched`) & spring gesture patterns allowed with attribution (MIT). |

## License Texts (Summary)

### MIT License (applicable to MIT-licensed references above)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

### Apache License, Version 2.0 (applicable to Apache-2.0 references above)

Licensed under the Apache License, Version 2.0 (the "License"); you may not use
these files except in compliance with the License. You may obtain a copy of the
License at <http://www.apache.org/licenses/LICENSE-2.0>. Unless required by
applicable law or agreed to in writing, software distributed under the License
is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
KIND, either express or implied.
