# hello world
loop = 0

def teleop():
    print("hello world!")
    while True:
        if Gamepad.get_value("button_a"):
            print("BUTTON A")
        if Gamepad.get_value("button_b"):
            print("BUTTON B")
        if Gamepad.get_value("button_x"):
            print("BUTTON X")
        if Gamepad.get_value("button_y"):
            print("BUTTON Y")
        if Gamepad.get_value("dpad_up"):
            print("DPAD UP ")
        if Gamepad.get_value("dpad_down"):
            print("DPAD DOWN")
        if Gamepad.get_value("dpad_left"):
            print("DPAD LEFT")
        if Gamepad.get_value("dpad_right"):
            print("DPAD RIGHT")
        if Gamepad.get_value("l_trigger"):
            print("LEFT TRIGGER")
        if Gamepad.get_value("r_trigger"):
            print("RIGHT TRIGGER")
        if Gamepad.get_value("l_bumper"):
            print("RIGHT BUMPER")
        if Gamepad.get_value("r_bumper"):
            print("LEFT BUMPER")
        if Gamepad.get_value("r_stick"):
            print("RIGHT STICK")
        if Gamepad.get_value("l_stick"):
            print("LEFT STICK")
        Robot.sleep(1.5)