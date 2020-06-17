from flask_login import current_user
from flask_wtf import FlaskForm
from wtforms import BooleanField, PasswordField, StringField, SubmitField
from wtforms.validators import DataRequired, Email, EqualTo, Length, ValidationError
from wtforms.widgets import PasswordInput

from portunus.models import User


class LoginForm(FlaskForm):
    email = StringField("Email", validators=[DataRequired(), Email()])
    password = PasswordField("Password", validators=[DataRequired(), Length(min=5)])
    remember = BooleanField("Remember Me")
    submit = SubmitField("Login")


class RegistrationForm(FlaskForm):
    username = StringField(
        "Username", validators=[DataRequired(), Length(min=2, max=20)]
    )
    email = StringField("Email", validators=[DataRequired(), Email()])
    password = PasswordField("Password", validators=[DataRequired()])
    confirm_password = PasswordField(
        "Confirm Password", validators=[DataRequired(), EqualTo("password")]
    )
    submit = SubmitField("Sign Up")

    def validate_username(self, username):
        user = User.query.filter_by(username=username.data).first()
        if user:
            raise ValidationError("User with that username already exists.")

    def validate_email(self, email):
        user = User.query.filter_by(email=email.data).first()
        if user:
            raise ValidationError("User with that email already exists.")


class UpdateAccountForm(FlaskForm):
    current_password = StringField(
        "Current Password",
        widget=PasswordInput(hide_value=True),
        validators=[DataRequired(), Length(min=6)],
    )
    new_username = StringField(
        "New Username", validators=[DataRequired(), Length(min=2, max=20)]
    )
    new_email = StringField("New Email", validators=[DataRequired(), Email()])
    new_password = StringField("New Password", widget=PasswordInput(hide_value=True))
    submit = SubmitField("Update")

    def validate_new_username(self, new_username):
        if new_username.data != current_user.username:
            user = User.query.filter_by(username=new_username.data).first()
            if user:
                raise ValidationError("User with that new username already exists.")

    def validate_new_email(self, new_email):
        if new_email.data != current_user.email:
            user = User.query.filter_by(email=new_email.data).first()
            if user:
                raise ValidationError("User with that new email already exists.")

    def validate_new_password(self, new_password):
        # If you're doing an update at all (non-zero text length), ensure it's at least length 6.
        if new_password.data:
            if len(new_password.data) < 6:
                raise ValidationError("New password is too short.")
