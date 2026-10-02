/**
 * What changed, in Lane's own words, kept here rather than fetched.
 *
 * A changelog that phones home is a changelog that leaves the Mac, which
 * is the one promise Lane makes. It ships with the version it describes.
 */
export const CHANGELOG: { version: string; date: string; lines: string[] }[] = [
  {
    version: '1.0.14',
    date: '2 October 2026',
    lines: [
      'Two short films on Today explaining what Lane is and how to drive it. They open in your browser: an embedded player would be a request to Google from a window that promises nothing leaves this Mac.',
    ],
  },
  {
    version: '1.0.13',
    date: '2 October 2026',
    lines: [
      'Updates install and Lane comes back. It was quitting to restart and never returning: the new copy found the old one still listening, decided it was a duplicate, and left.',
      'Restoring from a backup comes back the same way. It had the same fault.',
    ],
  },
  {
    version: '1.0.12',
    date: '2 October 2026',
    lines: [
      'A question that fails because your Mac has run out of memory is asked again, against a model that has been restarted. Before this, the first failure broke every question after it until Lane was quit.',
      'When it cannot be recovered, Lane says your Mac is out of memory rather than "Compute error", which is something you can actually do something about.',
    ],
  },
  {
    version: '1.0.11',
    date: '2 October 2026',
    lines: [
      'Answers come back in about a third of the time. They were long where two sentences were asked for, and every character costs a sixteenth of a second on a small Mac.',
      'Lane reads four memories before answering rather than six, and its own instructions are half the length. Both were read in full before a single word came back.',
      'Recall can be filmed with the rest of the app when LANE_UNPROTECTED is set. It stays hidden from screen sharing otherwise, as before.',
    ],
  },
  {
    version: '1.0.10',
    date: '29 September 2026',
    lines: [
      'Settings shows the version Lane actually is. It said 0.1.0 for nine releases, because it was written down rather than read.',
      'What changed, in Settings, shipped with the version it describes rather than fetched.',
      'An update that will not install says so and keeps saying it, instead of a message that disappears in four seconds.',
    ],
  },
  {
    version: '1.0.9',
    date: '29 September 2026',
    lines: [
      'Answers start in under a second instead of after a quarter of a minute. The model was reading the whole prompt again for every question.',
      'Every answer notes where its time went, so a slow one can be explained rather than guessed at.',
    ],
  },
  {
    version: '1.0.8',
    date: '29 September 2026',
    lines: [
      'A question now gets the whole machine rather than half of it while a memory is being written.',
      'Somebody else’s news stays theirs: a sale in a post you read is not a thing you sold, or owe.',
      'Dictation waits while you think. A pause of under two seconds used to end it.',
      'Captures and memories are no longer the same word for three different numbers.',
    ],
  },
  {
    version: '1.0.7',
    date: '29 September 2026',
    lines: ['Both model downloads are shown together beside Capture, instead of interrupting in the notch.'],
  },
  {
    version: '1.0.6',
    date: '29 September 2026',
    lines: [
      'Lane asks for the microphone. It never had, so macOS refused it in silence and dictation recorded an empty room.',
    ],
  },
  {
    version: '1.0.5',
    date: '28 September 2026',
    lines: [
      'Dictation was reading the sound from the wrong place in the file, so there was nothing to hear.',
      'A dictation that hears nothing now says why.',
    ],
  },
  {
    version: '1.0.4',
    date: '28 September 2026',
    lines: ['Lane installs without asking Apple anything, so it works on a plane.'],
  },
  {
    version: '1.0.3',
    date: '28 September 2026',
    lines: ['Opened from the disc it arrived on, Lane offers to move itself to Applications.'],
  },
  {
    version: '1.0.1',
    date: '27 September 2026',
    lines: [
      'The freeze after recording a meeting is gone.',
      'Dictation writes as you speak and goes in when you stop.',
      'The notch stays with you in full screen.',
    ],
  },
]
