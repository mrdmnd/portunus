import os
import secrets

from datetime import datetime
from flask import render_template, flash, redirect, url_for, request, send_from_directory
from flask_login import current_user, login_required
from portunus import db
from portunus.main.forms import UpdateAccountForm
from portunus.models import User, Route
from portunus.main import bp

@bp.before_app_request
def before_request():
    if current_user.is_authenticated:
        current_user.last_seen = datetime.utcnow()
        db.session.commit()

@bp.route('/favicon.ico')
def favicon():
    return send_from_directory(os.path.join(bp.root_path, 'static'), 'icons/favicon.ico')


@bp.route('/')
@bp.route('/index')
def index():
    return render_template('index.html')


def save_picture(form_picture):
    random_hex = secrets.token_hex(8)
    _, ext = os.path.splitext(form_picture.filename)
    picture_filename = random_hex + ext
    picture_path = os.path.join(bp.root_path, 'static/profile_pics', picture_filename)
    form_picture.save(picture_path)
    return picture_filename


# TODO(mrdmnd) - make this more like "edit_profile" and have a route for /user/<username> that anyone can click on to see details about each person
# go to https://blog.miguelgrinberg.com/post/the-flask-mega-tutorial-part-vi-profile-page-and-avatars and scroll to Profile Editor
@bp.route('/account', methods=["GET", "POST"])
@login_required
def account():
    form = UpdateAccountForm()
    if form.validate_on_submit():
        if form.picture.data:
            picture_file = save_picture(form.picture.data)
            current_user.image_file = picture_file

        # Update current username + email
        current_user.username = form.username.data
        current_user.email = form.email.data
        db.session.commit()
        flash('Your account has been updated.', 'success')
        return redirect(url_for('main.account'))
    elif request.method == "GET":
        form.username.data = current_user.username
        form.email.data = current_user.email

    image_file = url_for('static', filename='profile_pics/' + current_user.image_file)
    return render_template('account.html', title="Account", image_file=image_file, form=form)
