pipeline {
    agent any
    options {
        retry('three')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
