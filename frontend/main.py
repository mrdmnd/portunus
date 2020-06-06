import os

from datetime import datetime

from flask import Flask, render_template, send_from_directory, url_for, flash, redirect
from flask_sqlalchemy import SQLAlchemy
from forms import RegistrationForm, LoginForm

app = Flask(__name__)
app.config.from_object(__name__)
app.config['SECRET_KEY'] = 'e38425d0dcea98645bee3b0f3f3aaf5b'
app.config['SQLALCHEMY_DATABASE_URI'] = 'sqlite:///site.db'
db = SQLAlchemy(app)


class User(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    username = db.Column(db.String(20), unique=True, nullable=False)
    email = db.Column(db.String(255), unique=True, nullable=False)
    image_file = db.Column(db.String(20), nullable=False, default='default.jpg')
    password = db.Column(db.String(60), nullable=False)

    dungeon_routes = db.relationship('DungeonRoute', backref='author', lazy=True)

    def __repr__(self):
        return f"User('{self.username}', '{self.email}','{self.image_file}')"

class DungeonRoute(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    title = db.Column(db.String(100), nullable=False)
    date_created_utc = db.Column(db.DateTime, nullable=False, default=datetime.utcnow)
    content = db.Column(db.Text, nullable=False)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id'), nullable=False)

    def __repr__(self):
        return f"DungeonRoute('{self.title}', '{self.date_created_utc}')"

@app.route('/favicon.ico')
def favicon():
    return send_from_directory(os.path.join(app.root_path, 'static'), 'icons/favicon.ico')


@app.route('/')
@app.route('/home')
def home():
    return render_template('index.html', dungeon_routes = dungeon_routes)


@app.route('/about')
def about():
    return render_template('about.html')

@app.route('/register', methods=["GET", "POST"])
def register():
    form = RegistrationForm()
    if form.validate_on_submit():
        flash(f'Account created for user {form.username.data}!', 'success')
        return redirect(url_for('home'))

    return render_template('register.html', title='Register', form=form)

@app.route('/login', methods=["GET", "POST"])
def login():
    form = LoginForm()
    if form.validate_on_submit():
        if (form.email.data == 'admin@portunuspl.us' and form.password.data == 'password'):
            flash('Logged into account!', 'success')
            return redirect(url_for('home'))
        else:
            flash('Login unsuccessful. Please check Email and Password.', 'danger')
    return render_template('login.html', title='Login', form=form)




if __name__ == '__main__':
    app.run(debug=True)
