pipeline {
    agent any
    stages {
        stage('S') {
            input 'Go?'
            steps { echo 'x' }
        }
    }
}
