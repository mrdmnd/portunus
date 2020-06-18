import unittest

from datetime import datetime, timedelta
from config import Config
from portunus import create_app, db
from portunus.models import User, Route, Team, Character

class TestConfig(Config):
    TESTING = True
    SQLALCHEMY_DATABASE_URI = "sqlite://"

class UserModelCase(unittest.TestCase):
    def setUp(self):
        self.app = create_app(TestConfig)
        self.app_context = self.app.app_context()
        self.app_context.push()
        db.create_all()

    def tearDown(self):
        db.session.remove()
        db.drop_all()
        self.app_context.pop()

    def test_password_hashing(self):
        u = User(username="susan", email="susan@susan.com")
        u.set_password("cat")
        self.assertFalse(u.check_password("dog"))
        self.assertTrue(u.check_password("cat"))

    def test_character_membership(self):
        u1 = User(username="john", email="john@susan.com")
        u1.set_password("potato")
        u2 = User(username="susan", email="susan@susan.com")
        u2.set_password("tomato")

        c1 = Character(server="tich", name="jon", spec=25)
        c1.owner = u1
        c2 = Character(server="illidan", name="susyn", spec=15)
        c2.owner = u2

        t = Team(name="the move")
        t.owner = u1

        db.session.add(u1)
        db.session.add(u2)
        db.session.add(c1)
        db.session.add(c2)
        self.assertEqual(u1.owned_characters.all(), [c1])
        self.assertEqual(u2.owned_characters.all(), [c2])
        db.session.add(t)
        self.assertEqual(t.owner, u1)

        # Test Join
        t.add_character(c1)
        t.add_character(c2)
        db.session.commit()
        self.assertTrue(t.contains_character(c1))
        self.assertTrue(t.contains_character(c2))
        self.assertEqual(t.members.all(), [c1, c2])

        # Test Leave
        t.remove_character(c1)
        t.remove_character(c2)
        db.session.commit()
        self.assertFalse(t.contains_character(c1))
        self.assertFalse(t.contains_character(c2))
        self.assertEqual(t.members.all(), [])

if __name__ == "__main__":
    unittest.main(verbosity=2)
