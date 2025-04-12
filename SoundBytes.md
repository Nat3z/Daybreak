# Sound Effects Documentation

This document lists all sound effects used in the Daybreak robot control system, their filenames, and what they correspond to in the application.

## Button Sounds

| Sound Name | Filename | Description |
|------------|----------|-------------|
| button_south | button_south.mp3 | A/Cross/South button press |
| button_east | button_east.mp3 | B/Circle/East button press |
| button_west | button_west.mp3 | X/Square/West button press |
| button_north | button_north.mp3 | Y/Triangle/North button press |

## D-Pad Sounds

| Sound Name | Filename | Description |
|------------|----------|-------------|
| dpad_up | dpad_up.mp3 | D-pad up press |
| dpad_down | dpad_down.mp3 | D-pad down press |
| dpad_left | dpad_left.mp3 | D-pad left press |
| dpad_right | dpad_right.mp3 | D-pad right press |

## Trigger/Bumper Sounds

| Sound Name | Filename | Description |
|------------|----------|-------------|
| left_bumper | left_bumper.mp3 | Left bumper press |
| right_bumper | right_bumper.mp3 | Right bumper press |
| left_trigger | left_trigger.mp3 | Left trigger press |
| right_trigger | right_trigger.mp3 | Right trigger press |

## Analog Stick Sounds

| Sound Name | Filename | Description |
|------------|----------|-------------|
| stick_left_up | stick_left_up.mp3 | Left stick pushed up |
| stick_left_down | stick_left_down.mp3 | Left stick pushed down |
| stick_left_left | stick_left_left.mp3 | Left stick pushed left |
| stick_left_right | stick_left_right.mp3 | Left stick pushed right |
| stick_right_up | stick_right_up.mp3 | Right stick pushed up |
| stick_right_down | stick_right_down.mp3 | Right stick pushed down |
| stick_right_left | stick_right_left.mp3 | Right stick pushed left |
| stick_right_right | stick_right_right.mp3 | Right stick pushed right |

## Menu Button Sounds

| Sound Name | Filename | Description |
|------------|----------|-------------|
| select | select.mp3 | Select/Back button press |
| start | start.mp3 | Start button press |
| idle | idle.mp3 | Idle sound |

## System Sounds

| Sound Name | Filename | Description |
|------------|----------|-------------|
| startup | startup.mp3 | System startup sound |
| stop | stop.mp3 | System shutdown sound |

## File Format Requirements

- All sound files should be in MP3 format
- Recommended duration: 50-200ms for button/stick sounds, up to 500ms for system sounds
- Recommended bitrate: 128-192 kbps
- Recommended sample rate: 44.1kHz

## Sound Design Guidelines

1. Button sounds should be short and crisp
2. D-pad sounds should be distinct from regular button sounds
3. Stick movements should have subtle feedback
4. System sounds (startup/stop) should be more prominent but not jarring
5. All sounds should maintain a consistent volume level
6. Consider using different sound characteristics for different control groups (buttons vs sticks vs system) 