from datetime import datetime
from portunus import db, login_manager, bcrypt
from flask_login import UserMixin


@login_manager.user_loader
def load_user(user_id):
    return User.query.get(int(user_id))


class User(db.Model, UserMixin):
    # Direct fields
    id = db.Column(db.Integer, primary_key=True)
    username = db.Column(db.String(20), unique=True, index=True, nullable=False)
    email = db.Column(db.String(255), unique=True, index=True, nullable=False)
    date_created_utc = db.Column(db.DateTime, nullable=False, default=datetime.utcnow)
    password_hash = db.Column(db.String(60), nullable=False)
    biography = db.Column(
        db.String(140), default="No biography set for this user yet."
    )

    # Relationships
    owned_routes = db.relationship("Route", backref="owner", lazy=True)
    owned_characters = db.relationship("Character", backref="owner", lazy=True)
    owned_teams = db.relationship("Team", backref="owner", lazy=True)

    def __repr__(self):
        return f"User('{self.username}', '{self.email}')"

    def set_password(self, password):
        self.password_hash = bcrypt.generate_password_hash(password).decode("utf-8")

    def check_password(self, password):
        return bcrypt.check_password_hash(self.password_hash, password)


class Route(db.Model):
    # Direct fields
    id = db.Column(db.Integer, primary_key=True)
    owner_user_id = db.Column(db.Integer, db.ForeignKey("user.id"), nullable=False)
    title = db.Column(db.String(100), index=True, nullable=False)
    date_created_utc = db.Column(db.DateTime, nullable=False, default=datetime.utcnow)
    content = db.Column(db.Text, nullable=False)

    # Relationships
    # (none)

    def __repr__(self):
        return f"Route('{self.title}')"


team_members = db.Table(
    "team_members",
    db.Column("team_id", db.Integer, db.ForeignKey("team.id"), primary_key=True),
    db.Column(
        "character_id", db.Integer, db.ForeignKey("character.id"), primary_key=True
    ),
)


class Team(db.Model):
    # Direct fields
    id = db.Column(db.Integer, primary_key=True)
    owner_user_id = db.Column(db.Integer, db.ForeignKey("user.id"), nullable=False)
    name = db.Column(db.String(20), index=True, nullable=False)
    date_created_utc = db.Column(db.DateTime, nullable=False, default=datetime.utcnow)

    # Relationships
    members = db.relationship(
        "Character",
        secondary=team_members,
        lazy="subquery",
        backref=db.backref("teams", lazy=True)
    )

    def __repr__(self):
        return f"Team('{self.name}', owned by user_id '{self.owner_user_id}')"


class Character(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    owner_user_id = db.Column(db.Integer, db.ForeignKey("user.id"), nullable=False)
    server = db.Column(db.String(50), index=True, nullable=False)
    name = db.Column(db.String(20), index=True, nullable=False)
    spec = db.Column(db.Integer, index=True, nullable=False)

    # Relationships
    # (backhalf is already defined above in Team model)

    def join_team(self, team):
        if not self.is_member_of(team):
            self.team_members.append(team)

    def leave_team(self, team):
        if self.is_member_of(team):
            self.team_members.remove(team)
    
    def is_member_of(self, team):
        return self.team_members.filter(team.id == self.id).count() > 0

    def __repr__(self):
        return f"Character({self.server}-{self.name} (spec: {self.spec}) owned by user_id {self.owner_user_id})"

