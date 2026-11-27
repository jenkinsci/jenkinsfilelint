pipeline {
    agent any
    options {
        notAnOption()
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
