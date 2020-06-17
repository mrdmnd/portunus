from flask import render_template, redirect, url_for, flash, request
from flask_login import login_user, logout_user, current_user, login_required
from portunus import db
from portunus.auth import bp
from portunus.auth.forms import LoginForm, RegistrationForm, UpdateAccountForm
from portunus.models import User


@bp.route("/register", methods=["GET", "POST"])
def register():
    if current_user.is_authenticated:
        return redirect(url_for("main.index"))
    form = RegistrationForm()
    if form.validate_on_submit():
        user = User(username=form.username.data, email=form.email.data,)
        user.set_password(form.password.data)
        db.session.add(user)
        db.session.commit()
        flash("Your account has been created. You are now able to login!", "success")
        return redirect(url_for("auth.login"))

    return render_template("auth/register.html", title="Register", form=form)


@bp.route("/login", methods=["GET", "POST"])
def login():
    if current_user.is_authenticated:
        return redirect(url_for("main.index"))
    form = LoginForm()
    if form.validate_on_submit():
        # Check if any user exists with the submitted email.
        # If such a user exists and the passwords match, log them in.
        user = User.query.filter_by(email=form.email.data).first()
        if user and user.check_password(form.password.data):
            login_user(user, remember=form.remember.data)
            next_page = request.args.get("next")
            return redirect(next_page if next_page else url_for("main.index"))
        else:
            flash("Login unsuccessful. Please check Email and Password.", "danger")
    return render_template("auth/login.html", title="Login", form=form)


@bp.route("/logout")
def logout():
    logout_user()
    return redirect(url_for("main.index"))


# TODO(mrdmnd) - make this more like "edit_profile" and have a route for /user/<username> that anyone can click on to see details about each person
# go to https://blog.miguelgrinberg.com/post/the-flask-mega-tutorial-part-vi-profile-page-and-avatars and scroll to Profile Editor
@bp.route("/update_account", methods=["GET", "POST"])
@login_required
def update_account():
    form = UpdateAccountForm()
    if form.validate_on_submit():
        if not current_user.check_password(form.current_password.data):
            flash("Your password was incorrect.", "danger")
            return render_template(
                "update_account.html", title="Update Account", form=form
            )
        # Update current username + email
        current_user.username = form.new_username.data
        current_user.email = form.new_email.data
        current_user.biography = form.new_biography.data

        # Set new password only if they fill the field in.
        if form.new_password.data:
            current_user.set_password(form.new_password.data)
        db.session.commit()
        flash("Your account has been updated.", "success")
        return redirect(url_for("auth.update_account"))
    elif request.method == "GET":
        form.new_username.data = current_user.username
        form.new_email.data = current_user.email
        form.new_biography.data = current_user.biography

    return render_template("update_account.html", title="Update Account", form=form)
