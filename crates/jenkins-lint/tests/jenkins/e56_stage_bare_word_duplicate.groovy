pipeline {
    agent any
    stages {
        stage(Build) {
            steps { echo 'x' }
        }
        stage('Build') {
            steps { echo 'y' }
        }
    }
}
