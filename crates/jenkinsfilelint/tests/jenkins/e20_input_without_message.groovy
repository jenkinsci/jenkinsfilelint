pipeline {
    agent any
    stages {
        stage('S') {
            input {
                ok 'Yes'
            }
            steps { echo 'x' }
        }
    }
}
