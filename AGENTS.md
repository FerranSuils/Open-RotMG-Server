# Contributions

For gameplay or visual changes, capture the before state before editing, verify the after state through normal client gameplay, and prepare the 15-second demo described in `docs/videos.md`. Include the demo URL with the PR. Nonvisual changes need test evidence, not a staged gameplay clip.

Develop in a separate worktree with distinct port, state directory and browser profile; see the development instructions for launcher options. Copy the four testing recipes into scratch experiments before long gameplay runs. Verify observed outcomes, not merely dispatched input. Keep experimental helpers out of feature commits; expanding shared testing infrastructure requires explicit approval.

Keep recordings and generated media outside Git. Never capture credentials, other people's tabs or the desktop. Do not change hosting, branch protections or publication permissions as part of making a demo.
