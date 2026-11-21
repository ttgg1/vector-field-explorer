# Beamline mode for accelerator physics

Design proposal · 21 November 2026

A student has a beam source, a quadrupole and a screen. Can she narrow the horizontal spot by moving the magnet while keeping its strength fixed? Beamline lets her draw the apparatus, make a prediction, move one element and read the consequences in both transverse planes. She can keep the original spot as a reference, undo the move, or continue into free exploration.

## The first investigation

1. Open a planar line with a fixed source, one quadrupole and a downstream screen. Start with a reproducible beam of finite width and angular spread. Show the horizontal and vertical RMS spot widths.
2. Freeze the initial screen reading. Select the magnet and move it along the line. The source and screen retain their longitudinal positions. Both the distance to the magnet and the distance from the magnet to the screen change.
3. Watch the horizontal waist move on the envelope plot. Compare the horizontal and vertical widths against the frozen reading. A quadrupole that focuses horizontally defocuses vertically. Enable a second quadrupole with opposite polarity to explore the trade-off.
4. Select a ray and inspect the field, the transverse force and the prediction at the screen. Restore the starting line with Undo, or keep the variation. A guided activity can suggest a goal while leaving the controls available.

## Linked views and controls

| View | Readout | Edit or inspect |
| --- | --- | --- |
| Plan view | Source, labelled magnet bodies, drift spaces, screen and reference orbit. A sector dipole draws a curved reference path. | Select by pointer or keyboard-accessible element list. Drag along the reference path or enter an exact longitudinal position in metres. |
| Envelope | Horizontal and vertical RMS sizes against path length, with magnets aligned to the same longitudinal axis. | Toggle planes and individual rays, lock the display scale, freeze a baseline and move the shared inspection cursor. |
| Screen | Transverse spot, centroid, horizontal and vertical RMS widths, and sample count. | Move the screen or switch between position space and transverse phase space, such as horizontal displacement versus slope. |
| Magnet inspector | Length, signed strength, orientation and local transverse field. For a selected ray, show the force from its velocity, charge and local field. | Change strength or polarity, disable the field while retaining the element length, or compare charges. Label physical units and display scales. |

Store element positions along the reference path. Show whether a value refers to an entrance, centre or exit. Use centre position for the default drag handle. Moving a magnet takes space from one adjacent drift and gives it to the other. **Move downstream elements too** translates the chosen element and its successors together. Reject overlapping finite elements with a visible explanation and preserve the last valid layout. A bend edit can change downstream world coordinates while retaining longitudinal stations. Show the layout preview before committing the gesture. One drag creates one undo entry.

Let the student edit the species, reference momentum, centroid, beam widths, angular spreads and sample seed. Display units at entry and readout, for example metres, millimetres, milliradians, tesla, tesla per metre and GeV/c. Keep the generated source particles fixed during lattice edits so a difference on the screen measures the edit. Resampling is an explicit action. On compact layouts keep the selected screen reading visible during numeric edits.

## Physics and model boundaries

Track particles relative to a design orbit using path length $s$ and transverse state $(x,x',y,y')$, with slopes relative to the local reference direction. Define the local frame in the layout and inspector. Compose first-order maps in beam order. Drifts fill the gaps, quadrupoles act with opposite signs in the two planes, and a hard-edge sector dipole bends the reference orbit according to charge, reference momentum and field. Use a common field convention for the rendered poles, transport and force arrows. In a disabled bend, treat the length as straight drift and recompute downstream geometry.

Start with planar, monoenergetic single-pass transport in ideal static elements. The source momentum is an input and magnetic transport conserves its magnitude. Display the small-angle, linear-optics approximation near the result. RF acceleration, fringe fields, space charge, scattering and radiation are outside this first model. For finite magnets use length-aware transport; label thin-lens calculations as a separate teaching approximation. Compute screen widths from the same transported ensemble as the spot. If analytic covariance envelopes are added, label their origin and distinguish them from finite-sample RMS.

Beamline needs two transverse positions and slopes together. Give it a pure transport model that yields ray and screen samples indexed by $s$. The existing [ODE workbench](../../../src/ode.rs) handles time-based systems up to three components. Reuse scene persistence, undo, plots, selection and CAS presentation where their semantics fit. Give any animated traversal a clearly labelled distance cursor. Older saved scenes should keep opening in their original mode.

## From a magnet to the calculator

**Explain this trajectory** takes the selected ray at the lens entrance. In the horizontal thin-lens example let $x_0$ be its offset, $x'_0$ its incoming slope, $f$ the signed focal length and $L$ the drift to the screen. The slope just after the lens is $x'_0-x_0/f$, giving

$$
x_{\mathrm{screen}}=x_0+L\left(x'_0-\frac{x_0}{f}\right).
$$

For a parallel ray at $x_0=1$ mm, $f=1$ m and $L=1$ m, the predicted screen offset is zero. At $L=0.5$ m it is 0.5 mm. Offer both distances in the editable example. A beam with angular spread can have finite width even at $L=f$. When a magnet moves, refresh its entrance ray coordinates as well as the downstream drift. A linked prediction follows those changes; a frozen prediction retains its inputs. Let native CAS users inspect the substitution. The same analytic example and numeric comparison should be available on web.

## Delivery and acceptance

The first release includes the source, drifts, quadrupoles, a horizontal sector dipole and diagnostic screens. Start with the one-quadrupole investigation, then add the dipole and its curved layout before calling this scope complete.

- Move only the quadrupole. Read the position and strength back, confirm fixed source and screen stations, and compare both RMS widths with a frozen baseline. Undo restores the original positions and readings using the same sample seed.
- In a drift, a ray retains its slope and its offset advances by drift length times slope. In the thin-lens reference example, recover both screen offsets above. Reversing quadrupole polarity exchanges the focusing and defocusing planes for a symmetric injected beam.
- In the sector bend, the reference particle follows the drawn arc and stays centred in local coordinates. Reversing the field reverses the bend. Check force arrows against the displayed charge and velocity.
- On desktop and compact web layouts, select the same magnet by keyboard, edit an exact position, inspect the linked screen reading, save the experiment and reopen it with the same physical configuration.

## Follow-up investigations

**Measure momentum with a bend.** Introduce relative momentum offset and first-order dispersion. Compare three nearby momenta of the same species at one screen, then move the screen and measure the change in separation.

**Find the first aperture loss.** Add finite apertures, mark the first intercept of each lost ray and report transmission alongside the spot widths of surviving particles. Students can compare a narrow final spot with the fraction of the original beam arriving there.

The homework-to-scene workflow, linked CAS outputs and measurement sweeps are natural points of integration. Follow the relevant issues as those features land. [Homework workflow](https://github.com/ttgg1/vector-field-explorer/issues/212) · [CAS outputs](https://github.com/ttgg1/vector-field-explorer/issues/233) · [Measurement sweeps](https://github.com/ttgg1/vector-field-explorer/issues/234)

Physics reference: Wolfgang Hillert, *Transverse Linear Beam Dynamics I*, CERN Accelerator School. [Course and lecture materials](https://indico.cern.ch/event/1356988/contributions/5713241/)
