# The Most Important Rules for Writing Code

Your work should be _good_. That means, in this order:
1/ Minimal LoC to do the job
2/ Testabiltiy
3/ Strong observability
4/ Proper architecture that maximizes reliability and minimizes fallbacks

# Tips on Reducing LoC

When doing any work, consider - "Can I reduce LoC in mr PR?". Here are some ways to do that:
1/ Investigate the relevant code, and consider: "If I were reimplementing this architecture knowing what I know now, what can I delete and rewrite to reduce the LoC?"
2/ Before sending the PR, consider - "What tests did I only need to get here, and what can I delete?", or "What documentation did I see that was irrelevant, and can delete?"
3/ Do not add any documentation unless explcitly asked. When writing documentation, have a subagent delete 75% of it after, and simply confirm if that captures mostly what you wanted to get across.

# Contributions

For gameplay or visual changes, capture the before state before editing, verify the after state through normal client gameplay, and prepare the 15-second demo described in `docs/videos.md`. Use a PR when there is a good demo to show; otherwise maintainers may push validated changes directly to main. Nonvisual changes need test evidence, not a staged gameplay clip.

Develop in a separate worktree with distinct port, state directory and browser profile; see the development instructions for launcher options. Copy the four testing recipes into scratch experiments before long gameplay runs. Verify observed outcomes, not merely dispatched input. Keep experimental helpers out of feature commits; expanding shared testing infrastructure requires explicit approval.

Native browser QA must use a private virtual display/compositor; see `examples/testing/README.md`. CDP input and page-local focus must not activate host windows or switch the user's desktops. Never use host `Page.bringToFront`, target activation or desktop input as a rendering repair.

Keep recordings and generated media outside Git. Never capture credentials, other people's tabs or the desktop. Do not change hosting, branch protections or publication permissions as part of making a demo.

